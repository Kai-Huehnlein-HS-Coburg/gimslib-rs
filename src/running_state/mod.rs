mod egui_renderer;
mod texture_manager;

use std::mem::ManuallyDrop;
use std::sync::Arc;

use egui::TextureId;
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::*};
use windows::core::Interface;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::FrameData;
use crate::GPULib;
use crate::event::Event;
use crate::running_state::egui_renderer::EguiRenderer;
use crate::swapchain::Swapchain;
use crate::{App, FrameResources};

pub struct RunningFrameData {
    command_allocator: ID3D12CommandAllocator,
    command_list: ID3D12GraphicsCommandList10,
    fence: ID3D12Fence,
    event: Event,
    free_list: Vec<TextureId>,
}

pub struct RunningState<T> {
    lib: Arc<GPULib>,
    app: T,
    swapchain: Swapchain,
    frame_data: FrameData<RunningFrameData>,
    egui_renderer: EguiRenderer,
}

impl<T: App> RunningState<T> {
    pub fn new(
        window: Window,
        lib: Arc<GPULib>,
        app: T,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let window = Arc::new(window);
        let window_size = window.inner_size();
        let swapchain = Swapchain::new(
            lib.clone(),
            window.clone(),
            window_size.width,
            window_size.height,
            3,
        )?;

        let frame_data = FrameData::try_from_fn(1, |_| {
            let command_allocator = unsafe {
                lib.device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            }?;
            let command_list = unsafe {
                lib.device.CreateCommandList1(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    D3D12_COMMAND_LIST_FLAG_NONE,
                )
            }?;

            // All fences start in the signaled state
            let fence = unsafe { lib.device.CreateFence(1, D3D12_FENCE_FLAG_NONE) }?;

            let event = Event::new(false)?;

            Ok::<_, Box<dyn std::error::Error>>(RunningFrameData {
                command_allocator,
                command_list,
                fence,
                event,
                free_list: vec![],
            })
        })?;

        let egui_renderer = EguiRenderer::new(lib.clone(), &swapchain)?;

        Ok(RunningState {
            lib,
            app,
            swapchain,
            frame_data,
            egui_renderer,
        })
    }

    pub fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let RunningFrameData {
            command_allocator,
            command_list,
            fence,
            event,
            free_list,
        } = self.frame_data.get_current_mut();

        unsafe {
            // Wait for completion of the frame and immediately reset the fence
            fence.SetEventOnCompletion(1, **event)?;
            event.wait()?;
            fence.Signal(0)?;

            command_allocator.Reset()?;
            command_list.Reset(&*command_allocator, None)?;

            command_list.ResourceBarrier(&[transition(
                self.swapchain.current_render_target(),
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )]);
        }

        self.egui_renderer.record(|ctx| self.app.record_ui(ctx))?;
        *free_list = self.egui_renderer.apply_texture_delta(free_list)?;

        let (render_target_handle, render_target_handle_srgb) =
            self.swapchain.current_render_target_handle();

        let frame_resources = FrameResources {
            command_list,
            render_target: self.swapchain.current_render_target(),
            render_target_handle,
            render_target_handle_srgb,
            viewport: self.swapchain.viewport,
            scissor: self.swapchain.scissor,
        };
        self.app.draw(&frame_resources)?;
        self.egui_renderer.draw(&self.lib, &frame_resources);

        unsafe {
            command_list.ResourceBarrier(&[transition(
                self.swapchain.current_render_target(),
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);

            command_list.Close()?;
            self.lib
                .queue
                .ExecuteCommandLists(&[Some(command_list.cast()?)]);
            self.lib.queue.Signal(&*fence, 1)?;

            // Present operation will be appended to the main queue
            if self
                .swapchain
                .swapchain
                .Present(1, DXGI_PRESENT::default())
                .is_err()
            {
                return Err("DXGI present failed".into());
            }
        }

        self.frame_data.increment_frame();
        self.swapchain.window.request_redraw();

        Ok(())
    }

    pub fn event(&mut self, event: &WindowEvent) {
        // Let egui handle events and decide if they should be ignored from further processing
        if self.egui_renderer.handle_event(event) {
            return;
        }

        if let WindowEvent::Resized(_new_size) = event {
            // Resize swapchain
        }
    }
}

fn transition(
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: unsafe { std::mem::transmute_copy(resource) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
        ..Default::default()
    }
}
