use crate::ui::status::AiStatusState;
use eframe::egui;

pub fn show_ai_status_panel(ui: &mut egui::Ui, state: &AiStatusState) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("状态").strong());
            ui.separator();
            ui.label(&state.current);
        });
        let steps = state.latest_steps_text();
        if !steps.is_empty() {
            ui.small(steps);
        }
    });
}
