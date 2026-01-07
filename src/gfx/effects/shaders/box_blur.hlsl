// Box Blur Pixel Shader for Direct2D Custom Effect
// A simple custom blur implementation demonstrating multi-sample techniques.
// Compile with: fxc /T ps_4_0 /E main /Fo box_blur.cso box_blur.hlsl

#define D2D_INPUT_COUNT 1           // The pixel shader takes 1 input texture.
#define D2D_INPUT0_COMPLEX          // The first input is sampled in a complex manner: to calculate the output of a pixel,
                                    // the shader samples more than just the corresponding input coordinate.
#define D2D_REQUIRES_SCENE_POSITION // The pixel shader requires the SCENE_POSITION input.

// Input texture and sampler (provided by Direct2D)
Texture2D<float4> InputTexture : register(t0);
SamplerState InputSampler : register(s0);

// Constant buffer with effect properties
cbuffer EffectConstants : register(b0)
{
    float radius;     // Blur radius in pixels (0.0 = no blur, higher = more blur)
    float intensity;  // Blend factor: 0.0 = original, 1.0 = full blur
};

// Direct2D provides scene-to-input coordinate transformation
cbuffer d2dConstants : register(b1)
{
    float4 sceneToInput0;
};

// Box blur kernel - samples a square grid around the pixel and averages
float4 main(
    float4 clipSpaceOutput  : SV_POSITION,
    float4 sceneSpaceOutput : SCENE_POSITION,
    float4 texelSpaceInput0 : TEXCOORD0
) : SV_TARGET
{
    float2 basePos = texelSpaceInput0.xy;
    float2 texelSize = texelSpaceInput0.zw;  // Converts pixel offset to sample position offset
    
    // Early out for zero radius
    if (radius < 0.5)
    {
        return InputTexture.Sample(InputSampler, basePos);
    }
    
    // Calculate the kernel size based on radius
    // We use a fixed number of samples for performance, scaled by radius
    int sampleRadius = clamp((int)radius, 1, 8);  // Max 8 pixel radius (17x17 kernel max)
    
    float4 colorSum = float4(0, 0, 0, 0);
    float weightSum = 0.0;
    
    // Sample a square kernel around the current pixel
    for (int y = -sampleRadius; y <= sampleRadius; y++)
    {
        for (int x = -sampleRadius; x <= sampleRadius; x++)
        {
            float2 offset = float2((float)x, (float)y);
            float2 samplePos = basePos + offset * texelSize;
            
            float4 sampleColor = InputTexture.Sample(InputSampler, samplePos);
            colorSum += sampleColor;
            weightSum += 1.0;
        }
    }
    
    // Average the samples
    float4 blurred = colorSum / weightSum;
    
    // Sample the original color for blending
    float4 original = InputTexture.Sample(InputSampler, basePos);
    
    // Blend between original and blurred based on intensity
    float4 result;
    result.rgb = lerp(original.rgb, blurred.rgb, intensity);
    result.a = lerp(original.a, blurred.a, intensity);
    
    return result;
}
