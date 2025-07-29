use egui::{
    ClippedPrimitive, TextureId, TexturesDelta,
    epaint::{Primitive, Vertex},
};
use std::{ffi::c_void, mem::ManuallyDrop, sync::Arc};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D12::*,
        Dxgi::Common::{
            DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32_FLOAT,
            DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
        },
    },
    core::s,
};
use winit::{event::WindowEvent, window::Window};

use crate::{
    FrameResources,
    gpulib::GPULib,
    running_state::texture_manager::TextureManager,
    swapchain::Swapchain,
    vector_constant_buffer::{BufferLocation, VectorConstantBuffer},
};

struct EguiMesh {
    index_buffer: VectorConstantBuffer<u32>,
    vertex_buffer: VectorConstantBuffer<Vertex>,
    texture: u64,
}

/// Transforms from pixel values to D3D12 doordinate system
#[repr(C)]
#[repr(align(4))]
struct RootConstants {
    scale: [f32; 2],
    /// Added after scaling the vertices
    offset: [f32; 2],
}

pub struct EguiRenderer {
    context: egui::Context,
    egui_winit_state: egui_winit::State,
    lib: Arc<GPULib>,
    window: Arc<Window>,
    viewport_info: egui::ViewportInfo,
    texture_manager: TextureManager,
    // Output from this frame
    textures_delta: TexturesDelta,
    meshes: Vec<EguiMesh>,
    // Current count of meshes to draw.
    // Old buffers will not get discarded in case the next draw needs less of them.
    draw_count: usize,
    root_signature: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
}

impl EguiRenderer {
    pub fn new(
        lib: Arc<GPULib>,
        swapchain: &Swapchain,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let context = egui::Context::default();

        let egui_winit_state = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            &swapchain.window,
            Some(swapchain.window.scale_factor() as f32),
            swapchain.window.theme(),
            Some(16384),
        );

        let mut viewport_info = egui::ViewportInfo::default();
        egui_winit::update_viewport_info(&mut viewport_info, &context, &swapchain.window, true);

        let root_signature = Self::create_root_signature(&lib)?;
        let pipeline = Self::create_pipeline(&lib, root_signature.clone())?;

        let texture_manager = TextureManager::new(lib.clone())?;

        Ok(EguiRenderer {
            context,
            egui_winit_state,
            lib,
            window: swapchain.window.clone(),
            viewport_info,
            texture_manager,
            textures_delta: TexturesDelta::default(),
            meshes: vec![],
            draw_count: 0,
            root_signature,
            pipeline,
        })
    }

    /// If this function returns true, the event should be excluded from further processing
    pub fn handle_event(&mut self, event: &WindowEvent) -> bool {
        self.egui_winit_state
            .on_window_event(&self.window, event)
            .consumed
    }

    pub fn record(
        &mut self,
        ui_function: impl FnMut(&egui::Context),
    ) -> Result<(), Box<dyn std::error::Error>> {
        egui_winit::update_viewport_info(
            &mut self.viewport_info,
            &self.context,
            &self.window,
            false,
        );

        let mut raw_input = self.egui_winit_state.take_egui_input(&self.window);
        raw_input.viewport_id = egui::ViewportId::ROOT;

        let full_output = self.context.run(raw_input, ui_function);
        self.egui_winit_state
            .handle_platform_output(&self.window, full_output.platform_output);

        let primitives = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        self.update_primitives(&primitives)?;

        self.textures_delta = full_output.textures_delta;

        Ok(())
    }

    /// Creates, updates, and deletes the needed textures.
    /// `free_list` must be set to the return value of this function from the last completed frame.
    pub fn apply_texture_delta(
        &mut self,
        free_list: &[TextureId],
    ) -> Result<Vec<TextureId>, Box<dyn std::error::Error>> {
        self.texture_manager.free(free_list);
        self.texture_manager.set(&self.textures_delta.set)?;
        Ok(std::mem::take(&mut self.textures_delta.free))
    }

    pub fn draw(
        &self,
        _lib: &GPULib,
        FrameResources {
            command_list,
            render_target: _,
            render_target_handle,
            render_target_handle_srgb: _,
            viewport,
            scissor,
        }: &FrameResources,
    ) {
        let root_constants = RootConstants {
            offset: [-1.0, 1.0],
            scale: [2.0 / viewport.Width, -2.0 / viewport.Height],
        };
        let pointer: *const RootConstants = &root_constants;

        unsafe {
            command_list.OMSetRenderTargets(1, Some(render_target_handle), false, None);
            command_list.RSSetViewports(&[*viewport]);
            command_list.RSSetScissorRects(&[*scissor]);
            command_list.SetGraphicsRootSignature(&self.root_signature);
            command_list.SetPipelineState(&self.pipeline);
            command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            command_list.SetGraphicsRoot32BitConstants(
                0,
                size_of::<RootConstants>() as u32 / 4,
                pointer as *const c_void,
                0,
            );
        }
        for draw_index in 0..self.draw_count {
            let mesh = &self.meshes[draw_index];
            let texture = self
                .texture_manager
                .get_descriptor_heap(mesh.texture)
                .unwrap();

            unsafe {
                command_list.SetDescriptorHeaps(&[Some(texture.clone())]);
                command_list.SetGraphicsRootDescriptorTable(
                    1,
                    texture.GetGPUDescriptorHandleForHeapStart(),
                );
                command_list
                    .IASetVertexBuffers(0, Some(&[mesh.vertex_buffer.vertex_buffer_view()]));
                command_list.IASetIndexBuffer(Some(
                    &mesh.index_buffer.index_buffer_view(DXGI_FORMAT_R32_UINT),
                ));
                command_list.DrawIndexedInstanced(mesh.index_buffer.len() as u32, 1, 0, 0, 0);
            }
        }
    }

    fn update_primitives(
        &mut self,
        primitives: &[ClippedPrimitive],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (
            index,
            ClippedPrimitive {
                clip_rect: _,
                primitive,
            },
        ) in primitives.iter().enumerate()
        {
            let Primitive::Mesh(mesh) = primitive else {
                // Paint callbacks are not supported yet
                continue;
            };
            if self.meshes.len() < index + 1 {
                let index_buffer = VectorConstantBuffer::new(
                    self.lib.clone(),
                    mesh.indices.len(),
                    BufferLocation::GPU,
                )?;
                let vertex_buffer = VectorConstantBuffer::new(
                    self.lib.clone(),
                    mesh.vertices.len(),
                    BufferLocation::GPU,
                )?;
                let egui::TextureId::Managed(texture) = mesh.texture_id else {
                    // User textures are not supported yet
                    continue;
                };

                self.meshes.push(EguiMesh {
                    index_buffer,
                    vertex_buffer,
                    texture,
                });
            }
            let egui_mesh = &mut self.meshes[index];
            egui_mesh.index_buffer.upload(&mesh.indices)?;
            egui_mesh.vertex_buffer.upload(&mesh.vertices)?;
        }
        self.draw_count = primitives.len();

        Ok(())
    }

    fn create_root_signature(
        lib: &GPULib,
    ) -> Result<ID3D12RootSignature, Box<dyn std::error::Error>> {
        let mut root_blob_option = None;

        let sampler = D3D12_STATIC_SAMPLER_DESC {
            Filter: D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            AddressU: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressV: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 0,
            ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
            BorderColor: D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK,
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
            ShaderRegister: 0,
            RegisterSpace: 0,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        };

        let root_constant_size_32_bits = size_of::<RootConstants>() / 4;

        let texture_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: 0,
        };

        // Root parameter: 3x3 f32 matrix
        let parameters = [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                        Num32BitValues: root_constant_size_32_bits as u32,
                    },
                },
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &texture_range,
                    },
                },
            },
        ];

        unsafe {
            D3D12SerializeRootSignature(
                &D3D12_ROOT_SIGNATURE_DESC {
                    Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
                    NumParameters: parameters.len() as u32,
                    pParameters: parameters.as_ptr(),
                    NumStaticSamplers: 1,
                    pStaticSamplers: &sampler,
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
            "egui.hlsl",
            include_str!("egui.hlsl"),
            "vertex_main",
            "vs_6_5",
            &[],
            &[],
        )?;
        if !hassle_rs::fake_sign_dxil_in_place(&mut vertex_shader) {
            return Err("Failed to sign vertex shader".into());
        }
        let mut pixel_shader = hassle_rs::compile_hlsl(
            "egui.hlsl",
            include_str!("egui.hlsl"),
            "pixel_main",
            "ps_6_5",
            &[],
            &[],
        )?;
        if !hassle_rs::fake_sign_dxil_in_place(&mut pixel_shader) {
            return Err("Failed to sign pixel shader".into());
        }

        let input_element_descs = [
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: std::mem::offset_of!(Vertex, pos) as u32,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: std::mem::offset_of!(Vertex, uv) as u32,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: s!("COLOR"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                InputSlot: 0,
                AlignedByteOffset: std::mem::offset_of!(Vertex, color) as u32,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: input_element_descs.as_ptr(),
                NumElements: input_element_descs.len() as u32,
            },
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
                        BlendEnable: true.into(),
                        LogicOpEnable: false.into(),
                        SrcBlend: D3D12_BLEND_ONE,
                        DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
                        BlendOp: D3D12_BLEND_OP_ADD,
                        SrcBlendAlpha: D3D12_BLEND_ONE,
                        DestBlendAlpha: D3D12_BLEND_ONE,
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
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: false.into(),
                DepthFunc: D3D12_COMPARISON_FUNC_ALWAYS,
                ..Default::default()
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
}
