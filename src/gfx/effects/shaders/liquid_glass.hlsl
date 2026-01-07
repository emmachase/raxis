// Liquid Glass Effect - Ported from Shadertoy
// Creates a glass-like distortion with superellipse shape and glow
// Compile with: fxc /T ps_4_0 /E main /Fo liquid_glass.cso liquid_glass.hlsl

#define D2D_INPUT_COUNT 1
#define D2D_INPUT0_COMPLEX          // Samples from non-corresponding locations (distortion)
#define D2D_REQUIRES_SCENE_POSITION

// Input texture and sampler
Texture2D<float4> InputTexture : register(t0);
SamplerState InputSampler : register(s0);

// Constant buffer with effect properties
cbuffer EffectConstants : register(b0)
{
    float refraction;   // Refraction power (default 3.0)
    float glow;         // Glow intensity (default 0.35)
    float noise;        // Noise amount (default 0.06)
    float size;         // Effect size as fraction of smaller dimension (default 0.5)
};

// Direct2D coordinate transformation
cbuffer d2dConstants : register(b1)
{
    float4 sceneToInput0;
};

// Constants
static const float M_E = 2.718281828459045;
static const float M_TAU = 6.28318530718;

// Internal parameters
static const float u_powerFactor = 3.0;
static const float u_glowBias = 0.0;
static const float u_glowEdge0 = 0.06;
static const float u_glowEdge1 = 0.0;

// The f() function - controls distortion falloff
float f_func(float x) {
    float u_a = 0.7;
    float u_b = 2.3;
    float u_c = 5.2;
    float u_d = 6.9;
    return 1.0 - u_b * pow(u_c * M_E, -u_d * x - u_a);
}

// Simple noise function
float rand(float2 co) {
    return frac(sin(dot(co, float2(12.9898, 78.233))) * 43758.5453);
}

// Signed distance function for a superellipse
float sdSuperellipse(float2 p, float n, float r) {
    float2 p_abs = abs(p);
    float numerator = pow(p_abs.x, n) + pow(p_abs.y, n) - pow(r, n);
    
    float den_x = pow(p_abs.x, 2.0 * n - 2.0);
    float den_y = pow(p_abs.y, 2.0 * n - 2.0);
    
    float denominator = n * sqrt(den_x + den_y) + 1e-5;
    return numerator / denominator;
}

// Glow calculation
float Glow(float2 texCoord) {
    float2 centered = texCoord * 2.0 - 1.0;
    float radius = length(centered);
    float angleFactor = smoothstep(0.0, 0.9, radius);
    return sin(atan2(centered.y, centered.x) - 0.5) * angleFactor;
}

float4 main(
    float4 clipSpaceOutput  : SV_POSITION,
    float4 sceneSpaceOutput : SCENE_POSITION,
    float4 texelSpaceInput0 : TEXCOORD0
) : SV_TARGET
{
    float2 basePos = texelSpaceInput0.xy;
    float2 texelSize = texelSpaceInput0.zw;
    
    // Sample original color first
    float4 originalColor = InputTexture.Sample(InputSampler, basePos);
    
    // UV is in 0-1 range for the element
    float2 uv = basePos;
    float2 center = float2(0.5, 0.5);
    
    // Convert UV to -1 to 1 range centered on element
    // This maps the entire element to the superellipse bounds
    float effectSize = max(size, 0.01);
    float2 p = (uv - center) * 2.0 / effectSize;
    
    float r = 1.0;
    float d = sdSuperellipse(p, u_powerFactor, r);
    
    // Antialiasing: calculate edge blend factor
    // Use screen-space derivative for pixel-perfect AA width
    float2 gradient = float2(ddx(d), ddy(d));
    float aaWidth = length(gradient) * 1.5; // 1.5 pixels of AA
    float edgeBlend = 1.0 - smoothstep(-aaWidth, aaWidth, d);
    
    // Fully outside the shape (with AA margin) -> return original
    if (edgeBlend <= 0.0) {
        return originalColor;
    }
    
    float dist = max(-d, 0.001);
    float fval = f_func(dist);
    fval = max(fval, 0.001);
    float2 sampleP = p * pow(fval, refraction);
    
    // Scale back to UV space
    float2 targetUV = sampleP * effectSize / 2.0 + center;
    
    // Out of bounds -> use original color for this sample
    float4 effectColor;
    if (targetUV.x > 1.0 || targetUV.y > 1.0 || targetUV.x < 0.0 || targetUV.y < 0.0) {
        effectColor = originalColor;
    } else {
        // Sample distorted position
        effectColor = InputTexture.Sample(InputSampler, targetUV);
        
        // Add noise
        float noiseVal = (rand(basePos * 1000.0) - 0.5) * noise;
        effectColor.rgb += noiseVal;
        
        // Glow calculation - use normalized coordinates for consistent glow
        float2 localUV = (uv - center) / effectSize + 0.5;
        float glowFactor = Glow(localUV) * glow * smoothstep(u_glowEdge1, u_glowEdge0, dist) + 1.0 + u_glowBias;
        effectColor.rgb *= glowFactor;
    }
    
    // Blend between original and effect using AA edge factor
    float4 color = lerp(originalColor, effectColor, edgeBlend);
    
    // Ensure valid output
    color.rgb = clamp(color.rgb, 0.0, 1.0);
    
    return color;
}
