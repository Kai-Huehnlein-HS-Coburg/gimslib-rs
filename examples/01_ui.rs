use gimslib_rs::{FrameResources, gimslib::Lib};
use windows::Win32::Graphics::Direct3D12::*;

struct App {
    _root_signature: ID3D12RootSignature,
}

impl App {
    fn new(lib: &Lib) -> Self {
        let mut root_blob_option = None;
        unsafe {
            D3D12SerializeRootSignature(
                &D3D12_ROOT_SIGNATURE_DESC {
                    ..Default::default()
                },
                D3D_ROOT_SIGNATURE_VERSION_1,
                &mut root_blob_option,
                None,
            )
        }
        .unwrap();

        let root_blob = root_blob_option.expect("Failed to create root signature");
        let blob_data = unsafe {
            std::slice::from_raw_parts(
                root_blob.GetBufferPointer() as *const u8,
                root_blob.GetBufferSize(),
            )
        };

        let root_signature = unsafe { lib.device.CreateRootSignature(0, blob_data) }.unwrap();

        App {
            _root_signature: root_signature,
        }
    }
}

impl gimslib_rs::App for App {
    fn record_ui(&mut self, ctx: &egui::Context) {
        egui::Window::new("Window").show(ctx, |ui| ui.label("text"));
    }

    fn draw(
        &mut self,
        _lib: &gimslib_rs::gimslib::Lib,
        FrameResources {
            command_list,
            render_target: _,
            render_target_handle,
            render_target_handle_srgb: _,
            viewport: _,
            scissor: _,
        }: &FrameResources,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            command_list.ClearRenderTargetView(*render_target_handle, &[0.0, 0.0, 0.0, 1.0], None)
        };
        Ok(())
    }
}

fn main() {
    gimslib_rs::run_app(App::new).unwrap();
}
