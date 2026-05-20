impl AiVideoApp {
    fn show_debug_window(&mut self, ctx: &egui::Context) {
        if !self.settings.debug_mode { return; }
        let mut open = true;
        egui::Window::new("调试窗口")
            .resizable(true)
            .default_size(egui::vec2(720.0, 420.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("调试日志：{} 条", self.debug_log.len()));
                    if ui.button("清空").clicked() { self.debug_log.clear(); }
                });
                ui.separator();
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    for line in &self.debug_log {
                        ui.monospace(line);
                    }
                });
            });
        if !open { self.settings.debug_mode = false; }
    }
}