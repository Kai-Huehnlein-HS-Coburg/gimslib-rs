struct VertexShaderOutput
{
    float4 clip_space_position : SV_POSITION;
    float2 tex_coord: TEXCOORD;
    float4 color: COLOR;
};

struct RootConstants
{
    float2 scale;
    float2 offset;
};

ConstantBuffer<RootConstants> root_constants : register(b0);

Texture2D<float4> ui_texture : register(t0);
SamplerState      ui_sampler : register(s0);

VertexShaderOutput vertex_main(
    // Logical pixel coordinates (points).
    // (0,0) is the top left corner of the screen.
    float2 position : POSITION,
    // Normalized texture coordinates.
    // (0, 0) is the top left corner of the texture.
    // (1, 1) is the bottom right corner of the texture.
    float2 tex_coord : TEXCOORD,
    // sRGBA with premultiplied alpha
    // Alpha == 0 encodes additive blending
    float4 color: COLOR)
{
    float2 transformed = position * root_constants.scale + root_constants.offset;
    
    VertexShaderOutput output;
    output.clip_space_position = float4(transformed, 0.1, 1.0);
    output.tex_coord = tex_coord;
    output.color = color;
    return output;
}

float4 pixel_main(VertexShaderOutput input)
    : SV_TARGET
{
    return ui_texture.Sample(ui_sampler, input.tex_coord, 0) * input.color;
}