mod event;
pub mod frame_data;
pub mod gpulib;
pub mod running_state;
pub mod swapchain;

use std::cell::OnceCell;

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
    fn draw(
        &mut self,
        lib: &GPULib,
        frame_resources: &FrameResources,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// The winit application struct
struct AppRunner<T, F> {
    /// The function used to create the app once the window can be created
    app_creator: Option<F>,
    /// The state of the application, which gets populated when the winit resume method is executed
    running_state: OnceCell<RunningState<T>>,
}

impl<T, F> AppRunner<T, F>
where
    T: App,
    F: FnOnce(&GPULib) -> T,
{
    fn try_initialize_app(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let window = event_loop.create_window(WindowAttributes::default())?;
        let lib = GPULib::new()?;
        let app_creator = self
            .app_creator
            .take()
            .ok_or("Application cannot be initialized twice")?;
        let app = (app_creator)(&lib);
        let running_state = RunningState::new(window, lib, app)?;

        self.running_state
            .set(running_state)
            .map_err(|_| "Application cannot be initialized twice")?;

        Ok(())
    }
}

impl<T, F> winit::application::ApplicationHandler for AppRunner<T, F>
where
    T: App,
    F: FnOnce(&GPULib) -> T,
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

/// The app_creator function creates the user-defined application struct using a GPULib object,
/// which contains the basic Direct3D 12 structs.
pub fn run_app<T: App>(
    app_creator: impl FnOnce(&GPULib) -> T,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut AppRunner {
        app_creator: Some(app_creator),
        running_state: OnceCell::new(),
    })?;

    Ok(())
}
