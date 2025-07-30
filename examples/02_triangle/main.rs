use std::{mem::ManuallyDrop, sync::Arc};

use gimslib_rs::{AppConfig, FrameResources, gpulib::GPULib};
use windows::Win32::Graphics::{
    Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D12::*, Dxgi::Common::*,
};

struct App {
    root_signature: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    clear_color: [f32; 4],
}

impl App {
    fn new(lib: Arc<GPULib>) -> Self {
        let root_signature = create_root_signature(&lib).unwrap();
        let pipeline = create_pipeline(&lib, root_signature.clone()).unwrap();
        App {
            root_signature,
            pipeline,
            clear_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

fn create_root_signature(lib: &GPULib) -> Result<ID3D12RootSignature, Box<dyn std::error::Error>> {
    let mut root_blob_option = None;

    unsafe {
        D3D12SerializeRootSignature(
            &D3D12_ROOT_SIGNATURE_DESC::default(),
            D3D_ROOT_SIGNATURE_VERSION_1,
            &mut root_blob_option,
            None,
        )
    }?;

    let root_blob = root_blob_option.ok_or("Failed to create root signature")?;
    let blob_data = unsafe {
        std::slice::from_raw_parts(
            root_blob.GetBufferPointer() as *const u8,
            root_blob.GetBufferSize(),
        )
    };

    let root_signature = unsafe { lib.device.CreateRootSignature(0, blob_data) }?;

    Ok(root_signature)
}

fn create_pipeline(
    lib: &GPULib,
    root_signature: ID3D12RootSignature,
) -> Result<ID3D12PipelineState, Box<dyn std::error::Error>> {
    let mut vertex_shader = hassle_rs::compile_hlsl(
        "shader.hlsl",
        include_str!("shader.hlsl"),
        "VS_main",
        "vs_6_5",
        &[],
        &[],
    )?;
    if !hassle_rs::fake_sign_dxil_in_place(&mut vertex_shader) {
        return Err("Failed to sign vertex shader".into());
    }
    let mut pixel_shader = hassle_rs::compile_hlsl(
        "shader.hlsl",
        include_str!("shader.hlsl"),
        "PS_main",
        "ps_6_5",
        &[],
        &[],
    )?;
    if !hassle_rs::fake_sign_dxil_in_place(&mut pixel_shader) {
        return Err("Failed to sign pixel shader".into());
    }

    let mut pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC::default();
    pipeline_desc.pRootSignature = ManuallyDrop::new(Some(root_signature));
    pipeline_desc.VS = D3D12_SHADER_BYTECODE {
        pShaderBytecode: vertex_shader.as_ptr() as _,
        BytecodeLength: vertex_shader.len(),
    };
    pipeline_desc.PS = D3D12_SHADER_BYTECODE {
        pShaderBytecode: pixel_shader.as_ptr() as _,
        BytecodeLength: pixel_shader.len(),
    };
    pipeline_desc.RasterizerState = D3D12_RASTERIZER_DESC {
        FillMode: D3D12_FILL_MODE_SOLID,
        CullMode: D3D12_CULL_MODE_NONE,
        FrontCounterClockwise: false.into(),
        DepthBias: D3D12_DEFAULT_DEPTH_BIAS,
        DepthBiasClamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
        SlopeScaledDepthBias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
        DepthClipEnable: true.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
        ForcedSampleCount: 0,
        ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
    };
    pipeline_desc.BlendState.RenderTarget[0].RenderTargetWriteMask = 0b1111;
    pipeline_desc.SampleMask = u32::MAX;
    pipeline_desc.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    pipeline_desc.NumRenderTargets = 1;
    pipeline_desc.SampleDesc.Count = 1;
    pipeline_desc.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;

    let pipeline = unsafe { lib.device.CreateGraphicsPipelineState(&pipeline_desc)? };

    Ok(pipeline)
}

impl gimslib_rs::App for App {
    fn record_ui(&mut self, ctx: &egui::Context) {
        egui::Window::new("Window").show(ctx, |ui| {
            ui.label("Clear color:");
            ui.color_edit_button_rgba_unmultiplied(&mut self.clear_color)
        });
    }

    fn draw(&mut self, res: &FrameResources) -> Result<(), Box<dyn std::error::Error>> {
        let command_list = res.command_list;
        unsafe {
            command_list.ClearRenderTargetView(
                res.render_target_handle_srgb,
                &self.clear_color,
                None,
            );
            command_list.OMSetRenderTargets(1, Some(&res.render_target_handle), false, None);
            command_list.RSSetViewports(&[res.viewport]);
            command_list.RSSetScissorRects(&[res.scissor]);
            command_list.SetGraphicsRootSignature(&self.root_signature);
            command_list.SetPipelineState(&self.pipeline);
            command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            command_list.DrawInstanced(3, 1, 0, 0);
        }
        Ok(())
    }
}

fn main() {
    gimslib_rs::run_app(AppConfig::default(), App::new).unwrap();
}
