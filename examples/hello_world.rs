use imgui::*;

mod support;

fn main() {
    let system = support::init(file!());
    system.main_loop(move |_, ui| {
        ui.window("Hello world")
            .size([300.0, 110.0], Condition::FirstUseEver)
            .build(|| {
                ui.text("Hello world!");
                ui.text("こんにちは世界！");
                ui.text("This...is...imgui-rs!");
                ui.separator();
                let mouse_pos = ui.io().mouse_pos;
                ui.text(format!(
                    "Mouse Position: ({:.1},{:.1})",
                    mouse_pos[0], mouse_pos[1]
                ));
            });
    });
}
