use crate::widgets::renderer::ShadowCache;
use std::cell::RefCell;
use thiserror::Error;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    ID2D1Bitmap1, ID2D1Device6, ID2D1DeviceContext6, ID2D1Factory7, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::DirectComposition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual};
use windows::Win32::Graphics::DirectWrite::IDWriteFactory6;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIDevice4, IDXGIFactory7, IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows_core::{Error as WinError, IUnknown, Interface};
use std::mem::ManuallyDrop;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("Windows API error: {0}")]
    WindowsApi(#[from] WinError),

    #[error("Failed to create DXGI swapchain")]
    DxgiSwapchainCreationFailed(WinError),

    #[error("Failed to create DirectComposition visual")]
    DcompVisualCreationFailed(WinError),

    #[error("Failed to create DirectComposition target")]
    DcompTargetCreationFailed(WinError),

    #[error("Failed to resize swap chain buffers")]
    SwapChainResizeFailed(WinError),
}

pub type Result<T> = std::result::Result<T, DeviceError>;

/// Manages all Direct3D, Direct2D, DXGI, and DirectComposition device resources
/// for the rendering pipeline.
pub struct DeviceResources {
    // Rendering resources (recreated on resize/DPI change)
    pub solid_brush: Option<ID2D1SolidColorBrush>,
    pub d2d_target_bitmap: Option<ID2D1Bitmap1>,
    pub back_buffer: Option<ID3D11Texture2D>,
    pub dxgi_swapchain: Option<IDXGISwapChain1>,

    // Core factories and devices (persist across resize)
    pub dwrite_factory: IDWriteFactory6,
    pub dxgi_factory: IDXGIFactory7,
    pub dxgi_adapter: IDXGIAdapter,
    pub dxgi_device: IDXGIDevice4,
    pub d2d_device_context: ID2D1DeviceContext6,
    pub d2d_device: ID2D1Device6,
    pub d2d_factory: ID2D1Factory7,
    pub d3d_context: ID3D11DeviceContext,
    pub d3d_device: ID3D11Device,

    // DirectComposition objects
    pub dcomp_device: IDCompositionDevice,
    pub dcomp_target: IDCompositionTarget,
    pub dcomp_visual: Option<IDCompositionVisual>,

    // Shadow rendering cache
    pub shadow_cache: RefCell<ShadowCache>,
}

impl DeviceResources {
    /// Creates rendering resources (swap chain, render target, brush) for the given window dimensions.
    /// This is called during initialization and after window resize/DPI changes.
    pub fn create_device_resources(&mut self, hwnd: HWND, width: u32, height: u32) -> Result<()> {
        unsafe {
            // Create or reuse DXGI swap chain
            let dxgi_swapchain = match self.dxgi_swapchain {
                Some(ref dxgi_swapchain) => dxgi_swapchain,
                None => {
                    let swapchain_desc = DXGI_SWAP_CHAIN_DESC1 {
                        Width: width,
                        Height: height,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1, // Don't use multi-sampling
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: 2,
                        Scaling: DXGI_SCALING_STRETCH,
                        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                        ..Default::default()
                    };

                    let dxgi_swapchain: IDXGISwapChain1 = self
                        .dxgi_factory
                        .CreateSwapChainForComposition(
                            &self.d3d_device.cast::<IUnknown>().unwrap(),
                            &swapchain_desc,
                            None,
                        )
                        .map_err(DeviceError::DxgiSwapchainCreationFailed)?;

                    // Create DirectComposition visual and link to swap chain
                    let dcomp_visual = self
                        .dcomp_device
                        .CreateVisual()
                        .map_err(DeviceError::DcompVisualCreationFailed)?;
                    dcomp_visual
                        .SetContent(&dxgi_swapchain)
                        .map_err(DeviceError::DcompVisualCreationFailed)?;
                    self.dcomp_target
                        .SetRoot(&dcomp_visual)
                        .map_err(DeviceError::DcompTargetCreationFailed)?;
                    self.dcomp_visual = Some(dcomp_visual);

                    self.dxgi_swapchain = Some(dxgi_swapchain);
                    self.dxgi_swapchain
                        .as_ref()
                        .expect("Failed to create DXGI swapchain")
                }
            };

            // Get back buffer from swap chain
            let back_buffer = match self.back_buffer {
                Some(ref back_buffer) => back_buffer,
                None => {
                    let back_buffer: ID3D11Texture2D = dxgi_swapchain
                        .GetBuffer(0)
                        .map_err(DeviceError::DxgiSwapchainCreationFailed)?;
                    self.back_buffer = Some(back_buffer);
                    self.back_buffer
                        .as_ref()
                        .expect("Failed to create back buffer")
                }
            };

            // Create D2D render target bitmap from back buffer
            if self.d2d_target_bitmap.is_none() {
                let dpi = crate::current_dpi(hwnd) as f32;

                let bitmap_properties = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: dpi,
                    dpiY: dpi,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    colorContext: ManuallyDrop::new(None),
                };

                self.d2d_device_context.SetDpi(dpi, dpi);

                let d2d_target_bitmap = self
                    .d2d_device_context
                    .CreateBitmapFromDxgiSurface(
                        &back_buffer.cast::<IDXGISurface>().unwrap(),
                        Some(&bitmap_properties),
                    )
                    .map_err(DeviceError::DxgiSwapchainCreationFailed)?;

                self.d2d_device_context.SetTarget(&d2d_target_bitmap);
                self.d2d_target_bitmap = Some(d2d_target_bitmap);
            }

            // Create solid brush for rendering
            if self.solid_brush.is_none() {
                let rt = &self.d2d_device_context;
                let black = D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                };
                let brush = rt
                    .CreateSolidColorBrush(&black, None)
                    .map_err(DeviceError::DxgiSwapchainCreationFailed)?;
                self.solid_brush = Some(brush);
            }

            Ok(())
        }
    }

    /// Discards device-dependent resources (swap chain, render target, brush).
    /// Called before resize or when device needs to be recreated.
    pub fn discard_device_resources(&mut self) {
        self.solid_brush = None;
        self.back_buffer = None;
        self.d2d_target_bitmap = None;
        self.shadow_cache.borrow_mut().clear();

        unsafe {
            self.d2d_device_context.SetTarget(None);
            self.d3d_context.ClearState();
        }
    }

    /// Resizes the swap chain buffers to new dimensions.
    /// Must call `create_device_resources` after this to recreate render targets.
    pub fn resize_swap_chain(&mut self, width: u32, height: u32) -> Result<()> {
        unsafe {
            self.d2d_device_context.SetTarget(None);
            self.d3d_context.ClearState();

            self.d2d_target_bitmap = None;
            self.back_buffer = None;

            if let Some(ref mut swap_chain) = self.dxgi_swapchain {
                swap_chain
                    .ResizeBuffers(
                        0,
                        width,
                        height,
                        DXGI_FORMAT_UNKNOWN,
                        DXGI_SWAP_CHAIN_FLAG::default(),
                    )
                    .map_err(DeviceError::SwapChainResizeFailed)?;
            }
        }

        Ok(())
    }
}
