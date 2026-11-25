//! The window: the tab bar, the shared log view and the `eframe` hooks. The pages are in
//! `ui_pages`; every decision lives in the other modules.

use std::time::Duration;

use eframe::egui;

use crate::app::{App, Tab};
use crate::ui_pages;

/// How often the window redraws while a job or a folder check runs.
const BUSY_REPAINT: Duration = Duration::from_millis(200);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.is_busy() {
            ctx.request_repaint_after(BUSY_REPAINT);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Setup, "1  Setup");
                ui.selectable_value(&mut self.tab, Tab::Files, "2  Game folder");
                ui.selectable_value(&mut self.tab, Tab::Shaders, "3  Shaders");
                ui.selectable_value(&mut self.tab, Tab::Play, "4  Play");
            });
            ui.add_space(4.0);
        });
        if let Some(notice) = self.notice.clone() {
            egui::Panel::bottom("notice").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(notice);
                    if ui.small_button("dismiss").clicked() {
                        self.notice = None;
                    }
                });
            });
        }
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::Setup => ui_pages::setup(self, ui),
                Tab::Files => ui_pages::files(self, ui),
                Tab::Shaders => ui_pages::shaders(self, ui),
                Tab::Play => ui_pages::play(self, ui),
            });
        });
        ui_pages::confirm_clean(self, ui.ctx());
    }
}

/// The log of VIctory and of the launcher's steps, newest at the bottom.
pub fn log_view(app: &App, ui: &mut egui::Ui) {
    ui.separator();
    ui.label("Log");
    egui::ScrollArea::vertical()
        .id_salt("log")
        .max_height(260.0)
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in &app.log {
                ui.monospace(line);
            }
        });
}
