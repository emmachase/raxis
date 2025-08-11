use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_METRICS, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::UI::WindowsAndMessaging::STRSAFE_E_INSUFFICIENT_BUFFER;
use windows::core::Result;
use windows_numerics::Vector2;

use crate::gfx::RectDIP;

/// A widget that renders selectable text using DirectWrite and draws
/// the selection highlight using Direct2D.
///
/// It encapsulates selection state, hit-testing, and cached layout bounds
/// for cursor hit-testing.
pub struct SelectableText {
    // DirectWrite objects (shared/cloneable COM interfaces)
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    text: String,

    // layout
    bounds: RectDIP,
    layout: Option<IDWriteTextLayout>,

    // Selection state (UTF-16 code unit indices)
    selection_anchor: u32,
    selection_active: u32,
    is_dragging: bool,

    // Cached layout bounds in DIPs (for cursor hit-testing)
    metric_bounds: RectDIP,
}

impl SelectableText {
    pub fn new(
        dwrite_factory: IDWriteFactory,
        text_format: IDWriteTextFormat,
        text: String,
    ) -> Self {
        Self {
            dwrite_factory,
            text_format,
            text,
            bounds: RectDIP::default(),
            layout: None,
            selection_anchor: 0,
            selection_active: 0,
            is_dragging: false,
            metric_bounds: RectDIP::default(),
        }
    }

    /// Build a text layout for the given text and maximum size in DIPs.
    pub fn build_text_layout(&mut self) -> Result<()> {
        unsafe {
            let wtext: Vec<u16> = self.text.encode_utf16().collect();
            let layout = self.dwrite_factory.CreateTextLayout(
                &wtext,
                &self.text_format,
                self.bounds.width_dip,
                self.bounds.height_dip,
            )?;
            self.layout = Some(layout);
            Ok(())
        }
    }

    pub fn update_bounds(&mut self, bounds: RectDIP) -> Result<()> {
        if bounds != self.bounds {
            self.bounds = bounds;
            self.build_text_layout()?;

            unsafe {
                let mut metrics = DWRITE_TEXT_METRICS::default();
                self.layout
                    .as_ref()
                    .expect("layout not built")
                    .GetMetrics(&mut metrics)?;
                self.metric_bounds = RectDIP {
                    x_dip: metrics.left,
                    y_dip: metrics.top,
                    width_dip: metrics.width,
                    height_dip: metrics.height,
                };
            }
        }

        Ok(())
    }

    /// Draw selection highlight behind the text for the currently selected range.
    /// Restores the brush color to black afterwards to match typical text color.
    fn draw_selection(
        &self,
        layout: &IDWriteTextLayout,
        rt: &ID2D1HwndRenderTarget,
        brush: &ID2D1SolidColorBrush,
    ) -> Result<()> {
        unsafe {
            let sel_start = self.selection_anchor.min(self.selection_active);
            let sel_end = self.selection_anchor.max(self.selection_active);
            let sel_len = sel_end.saturating_sub(sel_start);
            if sel_len == 0 {
                return Ok(());
            }

            let mut needed: u32 = 0;
            match layout.HitTestTextRange(sel_start, sel_len, 0.0, 0.0, None, &mut needed) {
                Ok(()) => {
                    // Nothing visible to draw
                    return Ok(());
                }
                Err(e) if e.code() == STRSAFE_E_INSUFFICIENT_BUFFER => {
                    let capacity = needed.max(1);
                    loop {
                        let mut runs = vec![DWRITE_HIT_TEST_METRICS::default(); capacity as usize];
                        let mut actual: u32 = 0;
                        match layout.HitTestTextRange(
                            sel_start,
                            sel_len,
                            0.0,
                            0.0,
                            Some(&mut runs),
                            &mut actual,
                        ) {
                            Ok(()) => {
                                // Selection color (light blue)
                                brush.SetColor(&D2D1_COLOR_F {
                                    r: 0.2,
                                    g: 0.4,
                                    b: 1.0,
                                    a: 0.35,
                                });
                                for m in runs.iter().take(actual as usize) {
                                    let rect = D2D_RECT_F {
                                        left: m.left,
                                        top: m.top,
                                        right: m.left + m.width,
                                        bottom: m.top + m.height,
                                    };
                                    rt.FillRectangle(&rect, brush);
                                }
                                // Restore brush to black for drawing text
                                brush.SetColor(&D2D1_COLOR_F {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                });
                                break Ok(());
                            }
                            Err(e) => break Err(e),
                        }
                    }
                }
                Err(e) => Err(e),
            }
        }
    }

    pub fn draw(&self, rt: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush) -> Result<()> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");

            self.draw_selection(layout, rt, brush)?;

            rt.DrawTextLayout(
                Vector2 { X: 0.0, Y: 0.0 },
                layout,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
            Ok(())
        }
    }

    /// Hit-test a point in DIPs against the given text with the provided layout bounds.
    pub fn hit_test_index(&self, x_dip: f32, y_dip: f32) -> Result<u32> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");
            let mut trailing = windows::core::BOOL(0);
            let mut inside = windows::core::BOOL(0);
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestPoint(x_dip, y_dip, &mut trailing, &mut inside, &mut metrics)?;

            let mut idx = if trailing.as_bool() {
                metrics.textPosition.saturating_add(metrics.length)
            } else {
                metrics.textPosition
            };
            let total_len = self.text.encode_utf16().count() as u32;
            if idx > total_len {
                idx = total_len;
            }
            Ok(idx)
        }
    }

    // Drag/select helpers
    pub fn begin_drag(&mut self, idx: u32) {
        self.selection_anchor = idx;
        self.selection_active = idx;
        self.is_dragging = true;
    }

    pub fn update_drag_index(&mut self, idx: u32) -> bool {
        if self.is_dragging && idx != self.selection_active {
            self.selection_active = idx;
            return true;
        }
        false
    }

    pub fn end_drag(&mut self, idx: u32) {
        self.selection_active = idx;
        self.is_dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    pub fn metric_bounds(&self) -> RectDIP {
        self.metric_bounds
    }
}
