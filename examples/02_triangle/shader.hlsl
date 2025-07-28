static const float3 vertices[] = {{0.0f, 0.25f, 0.5f}, {0.25f, -0.25f, 0.5f}, {-0.25f, -0.25f, 0.5f}};
static const float3 colors[] = {{1.0f, 0.0f, 0.0f}, {0.0f, 1.0f, 0.0f}, {0.0f, 0.0f, 1.0f}};

struct VertexShaderOutput
{
    float4 position : SV_POSITION;
    float4 color : COLOR;
};


VertexShaderOutput VS_main(uint i : SV_VertexID)
{
    VertexShaderOutput output;
    output.position = float4(vertices[i], 1.0f);  
    output.color = float4(colors[i], 1.0f);  
    return output;
}

float4 PS_main(VertexShaderOutput input) : SV_TARGET
{
    return input.color;
}
