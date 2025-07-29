mod event;
pub mod frame_data;
pub mod gpulib;
mod running_state;
mod swapchain;
pub mod vector_constant_buffer;

use std::{cell::OnceCell, sync::Arc};

use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::Direct3D12::*,
        UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW},
    },
    core::{HSTRING, h},
};
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowAttributes,
};

use frame_data::FrameData;
use gpulib::GPULib;
use running_state::RunningState;

pub struct FrameResources<'a> {
    pub command_list: &'a ID3D12GraphicsCommandList10,
    pub render_target: &'a ID3D12Resource,
    pub render_target_handle: D3D12_CPU_DESCRIPTOR_HANDLE,
    pub render_target_handle_srgb: D3D12_CPU_DESCRIPTOR_HANDLE,
    pub viewport: D3D12_VIEWPORT,
    pub scissor: RECT,
}

pub trait App {
    fn record_ui(&mut self, ctx: &egui::Context);
    fn draw(&mut self, frame_resources: &FrameResources) -> Result<(), Box<dyn std::error::Error>>;
}

/// The winit application struct
struct AppRunner<T, F> {
    /// The function used to create the app once the window can be created
    app_creator: Option<F>,
    /// The state of the application, which gets populated when the winit resume method is executed
    running_state: OnceCell<RunningState<T>>,
    /// Settings like window title
    app_config: AppConfig,
}

impl<T, F> AppRunner<T, F>
where
    T: App,
    F: FnOnce(Arc<GPULib>) -> T,
{
    fn try_initialize_app(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let window = event_loop.create_window(
            WindowAttributes::default()
                .with_title(self.app_config.window_title.as_str())
                .with_inner_size(self.app_config.window_size),
        )?;
        let lib = Arc::new(GPULib::new()?);
        let app_creator = self
            .app_creator
            .take()
            .ok_or("Application cannot be initialized twice")?;
        let app = (app_creator)(lib.clone());
        let running_state = RunningState::new(window, lib, app, self.app_config.frame_count)?;

        self.running_state
            .set(running_state)
            .map_err(|_| "Application cannot be initialized twice")?;

        Ok(())
    }
}

impl<T, F> winit::application::ApplicationHandler for AppRunner<T, F>
where
    T: App,
    F: FnOnce(Arc<GPULib>) -> T,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Only create the running state once
        if self.running_state.get().is_some() {
            return;
        }

        if let Err(error) = self.try_initialize_app(event_loop) {
            let error_message = format!("{}", error);
            println!("Error while initializing application:\n{}", error_message);
            unsafe {
                MessageBoxW(
                    None,
                    &HSTRING::from(error_message),
                    h!("Error while initializing application"),
                    MB_ICONERROR,
                )
            };
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(running_state) = self.running_state.get_mut() {
                    running_state.draw().unwrap();
                }
            }
            event => {
                if let Some(running_state) = self.running_state.get_mut() {
                    running_state.event(&event);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WindowSize {
    /// Width and height in display pixels
    Physical(u32, u32),
    /// Width and height in logical pixels, respecting scaling factors
    Logical(u32, u32),
}

impl From<WindowSize> for winit::dpi::Size {
    fn from(val: WindowSize) -> Self {
        match val {
            WindowSize::Physical(width, height) => {
                winit::dpi::PhysicalSize { width, height }.into()
            }
            WindowSize::Logical(width, height) => winit::dpi::LogicalSize { width, height }.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Window title
    pub window_title: String,
    /// Width and height of the drawing area
    pub window_size: WindowSize,
    /// Number of swapchain frames in flight
    pub frame_count: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            window_title: "gimslib-rs window".to_string(),
            window_size: WindowSize::Logical(1024, 768),
            frame_count: 2,
        }
    }
}

/// The app_creator function creates the user-defined application struct using a GPULib object,
/// which contains the basic Direct3D 12 structs.
pub fn run_app<T: App>(
    app_config: AppConfig,
    app_creator: impl FnOnce(Arc<GPULib>) -> T,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut AppRunner {
        app_creator: Some(app_creator),
        running_state: OnceCell::new(),
        app_config,
    })?;

    Ok(())
}
