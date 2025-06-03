use std::{ffi::c_void, sync::Arc};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::Interface;
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use crate::Lib;

pub struct Swapchain {
    pub render_target_heap: ID3D12DescriptorHeap,
    pub render_targets: Vec<ID3D12Resource>,
    pub swapchain: IDXGISwapChain4,
    pub window: Arc<Window>,
    pub viewport: D3D12_VIEWPORT,
    pub scissor: RECT,
    lib: Arc<Lib>,
}

impl Swapchain {
    pub fn new(
        lib: Arc<Lib>,
        window: Arc<Window>,
        width: u32,
        height: u32,
        frame_count: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let RawWindowHandle::Win32(window_handle) = window.window_handle()?.as_raw() else {
            return Err("Failed to get Win32 window handle".into());
        };

        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: frame_count,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            Scaling: DXGI_SCALING_STRETCH,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };

        let swapchain: IDXGISwapChain4 = unsafe {
            lib.factory.CreateSwapChainForHwnd(
                &lib.queue,
                HWND(window_handle.hwnd.get() as *mut c_void),
                &desc,
                None,
                None,
            )
        }?
        .cast()?;

        let render_target_heap: ID3D12DescriptorHeap = unsafe {
            lib.device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    NumDescriptors: frame_count,
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    ..Default::default()
                })
        }?;

        let rtv_descriptor_size = unsafe {
            lib.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        };

        let render_targets: Vec<ID3D12Resource> = (0..frame_count as usize)
            .map(|frame| {
                let render_target = unsafe { swapchain.GetBuffer(frame.try_into()?) }?;

                unsafe {
                    lib.device.CreateRenderTargetView(
                        &render_target,
                        None,
                        D3D12_CPU_DESCRIPTOR_HANDLE {
                            ptr: render_target_heap.GetCPUDescriptorHandleForHeapStart().ptr
                                + frame * rtv_descriptor_size as usize,
                        },
                    )
                };

                Ok(render_target)
            })
            .collect::<Result<_, Box<dyn std::error::Error>>>()?;

        let viewport = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };

        let scissor = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };

        Ok(Swapchain {
            lib,
            swapchain,
            window,
            render_target_heap,
            viewport,
            scissor,
            render_targets,
        })
    }

    pub fn current_render_target(&self) -> &ID3D12Resource {
        let index = unsafe { self.swapchain.GetCurrentBackBufferIndex() } as usize;
        &self.render_targets[index]
    }

    pub fn current_render_target_handle(&self) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        let increment = unsafe {
            self.lib
                .device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        } as usize;
        D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: unsafe {
                self.render_target_heap
                    .GetCPUDescriptorHandleForHeapStart()
                    .ptr
                    + increment * self.swapchain.GetCurrentBackBufferIndex() as usize
            },
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            // Wait for queue idle
            let fence: ID3D12Fence = self
                .lib
                .device
                .CreateFence(0, D3D12_FENCE_FLAG_NONE)
                .unwrap();
            self.lib.queue.Signal(&fence, 1).unwrap();
            self.lib.queue.Wait(&fence, 1).unwrap();
        }
    }
}
