use std::{mem::ManuallyDrop, sync::Arc};

use gimslib_rs::{FrameResources, frame_data::FrameData, gpulib::GPULib};
use nalgebra_glm::Mat4;
use windows::Win32::Graphics::{
    Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D12::*, Dxgi::Common::*,
};

#[repr(C)]
struct PerFrameConstants {
    rotation: Mat4,
}

struct App {
    root_signature: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    frame_data: FrameData<ID3D12Resource>,
    start_time: std::time::Instant,
    clear_color: [f32; 4],
}

impl App {
    fn new(lib: Arc<GPULib>) -> Self {
        let root_signature = create_root_signature(&lib).unwrap();
        let pipeline = create_pipeline(&lib, root_signature.clone()).unwrap();
        let frame_data = FrameData::from_fn(2, |_| {
            create_constant_buffer(&lib, size_of::<PerFrameConstants>()).unwrap()
        });
        let start_time = std::time::Instant::now();

        App {
            root_signature,
            pipeline,
            frame_data,
            start_time,
            clear_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

fn create_root_signature(lib: &GPULib) -> Result<ID3D12RootSignature, Box<dyn std::error::Error>> {
    let parameter = D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            Descriptor: D3D12_ROOT_DESCRIPTOR {
                ShaderRegister: 0,
                RegisterSpace: 0,
            },
        },
    };

    let mut root_blob_option = None;
    unsafe {
        D3D12SerializeRootSignature(
            &D3D12_ROOT_SIGNATURE_DESC {
                NumParameters: 1,
                pParameters: &parameter,
                ..Default::default()
            },
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

    let pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        pRootSignature: ManuallyDrop::new(Some(root_signature)),
        VS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: vertex_shader.as_ptr() as _,
            BytecodeLength: vertex_shader.len(),
        },
        PS: D3D12_SHADER_BYTECODE {
            pShaderBytecode: pixel_shader.as_ptr() as _,
            BytecodeLength: pixel_shader.len(),
        },
        RasterizerState: D3D12_RASTERIZER_DESC {
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
        },
        BlendState: D3D12_BLEND_DESC {
            AlphaToCoverageEnable: false.into(),
            IndependentBlendEnable: false.into(),
            RenderTarget: [
                D3D12_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: false.into(),
                    LogicOpEnable: false.into(),
                    SrcBlend: D3D12_BLEND_ONE,
                    DestBlend: D3D12_BLEND_ZERO,
                    BlendOp: D3D12_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D12_BLEND_ONE,
                    DestBlendAlpha: D3D12_BLEND_ZERO,
                    BlendOpAlpha: D3D12_BLEND_OP_ADD,
                    RenderTargetWriteMask: 0b1111,
                    ..Default::default()
                },
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
                D3D12_RENDER_TARGET_BLEND_DESC::default(),
            ],
        },
        SampleMask: u32::MAX,
        PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        NumRenderTargets: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        RTVFormats: [
            DXGI_FORMAT_R8G8B8A8_UNORM,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
        ],
        ..Default::default()
    };
    let pipeline = unsafe { lib.device.CreateGraphicsPipelineState(&pipeline_desc)? };

    Ok(pipeline)
}

fn create_constant_buffer(
    lib: &GPULib,
    size_bytes: usize,
) -> Result<ID3D12Resource, Box<dyn std::error::Error>> {
    let heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_GPU_UPLOAD,
        ..Default::default()
    };

    let resource_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: size_bytes as u64,
        Height: 1,
        DepthOrArraySize: 1,
        Alignment: 0,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAGS::default(),
    };

    let mut resource_option = None;
    unsafe {
        lib.device.CreateCommittedResource(
            &heap_properties,
            D3D12_HEAP_FLAGS::default(),
            &resource_desc,
            D3D12_RESOURCE_STATE_COMMON,
            None,
            &mut resource_option,
        )
    }?;

    resource_option.ok_or("Failed to create constant buffer".into())
}

impl App {
    fn update_constant_buffer<T>(&self, contents: T) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let mut pointer = std::ptr::null_mut();
            let constant_buffer = self.frame_data.get_current();
            constant_buffer.Map(0, None, Some(&mut pointer))?;
            let pointer_t = pointer as *mut T;
            *pointer_t = contents;
            constant_buffer.Unmap(0, None);
        }
        Ok(())
    }
}

impl gimslib_rs::App for App {
    fn record_ui(&mut self, ctx: &egui::Context) {
        egui::Window::new("Window").show(ctx, |ui| {
            ui.label("Clear color:");
            ui.color_edit_button_rgba_unmultiplied(&mut self.clear_color);
        });
    }

    fn draw(
        &mut self,
        res: &FrameResources,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let angle_radians = (std::time::Instant::now() - self.start_time).as_secs_f64()
            % (2.0 * std::f64::consts::PI);
        let contents = PerFrameConstants {
            rotation: nalgebra_glm::rotation(angle_radians as f32, &[0.0, 0.0, 1.0].into()),
        };
        self.update_constant_buffer(contents)?;

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
            command_list.SetGraphicsRootConstantBufferView(
                0,
                self.frame_data.get_current().GetGPUVirtualAddress(),
            );
            command_list.DrawInstanced(3, 1, 0, 0);
        }
        self.frame_data.increment_frame();
        Ok(())
    }
}

fn main() {
    gimslib_rs::run_app(App::new).unwrap();
}
