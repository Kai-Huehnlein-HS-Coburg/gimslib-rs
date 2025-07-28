use gimslib_rs::FrameResources;

struct App {
    clear_color: [f32; 4],
}

impl Default for App {
    fn default() -> Self {
        App {
            clear_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl gimslib_rs::App for App {
    fn record_ui(&mut self, ctx: &egui::Context) {
        egui::Window::new("Window").show(ctx, |ui| {
            ui.label("Clear color:");
            ui.color_edit_button_rgba_unmultiplied(&mut self.clear_color)
        });
    }

    fn draw(
        &mut self,
        _lib: &gimslib_rs::gimslib::GPULib,
        res: &FrameResources,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            res.command_list.ClearRenderTargetView(
                res.render_target_handle_srgb,
                &self.clear_color,
                None,
            )
        };
        Ok(())
    }
}

fn main() {
    gimslib_rs::run_app(|_| App::default()).unwrap();
}
