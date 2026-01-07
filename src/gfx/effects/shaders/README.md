# Custom Effect Shaders

This directory contains HLSL pixel shaders for the built-in custom effects.

## Automatic Compilation

The shaders are automatically compiled during the build process via `build.rs`.
The build script locates `fxc.exe` from the Windows SDK and compiles all `.hlsl`
files to `.cso` bytecode.

### Prerequisites

- Windows SDK (includes fxc.exe)
- The build script automatically finds fxc.exe in standard Windows SDK locations

### Manual Override

If you need to use a specific fxc.exe location, set the `FXC_PATH` environment variable:

```batch
set FXC_PATH=C:\path\to\fxc.exe
cargo build
```

## Creating Custom Effects

To create your own effect:

1. Write an HLSL pixel shader following the pattern in these files
2. Compile it to a .cso file
3. Create a struct implementing `PixelShaderEffect` trait
4. Use `include_bytes!("path/to/shader.cso")` in the metadata

See the `builtins.rs` file for examples of effect implementations.

## Shader Structure

Each shader should:

1. Declare input texture and sampler:
   ```hlsl
   Texture2D<float4> InputTexture : register(t0);
   SamplerState InputSampler : register(s0);
   ```

2. Declare effect properties in a constant buffer:
   ```hlsl
   cbuffer EffectConstants : register(b0)
   {
       float myProperty;
   };
   ```

3. Implement the main function with proper semantics:
   ```hlsl
   float4 main(
        float4 clipSpaceOutput  : SV_POSITION,
        float4 sceneSpaceOutput : SCENE_POSITION,
        float4 texelSpaceInput0 : TEXCOORD0
   ) : SV_TARGET
   {
       float4 color = InputTexture.Sample(InputSampler, texCoord.xy);
       // Transform color...
       return color;
   }
   ```
