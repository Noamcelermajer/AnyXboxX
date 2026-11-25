//! The four pages of the window: setup checklist, game folder, shader preloading, play.

use eframe::egui::{self, Color32, RichText};

use crate::app::{App, Verify};
use crate::setup::State;
use crate::verify::{self, Problem};
use crate::{session, ui};
use anyxboxx::run::XgMode;

const OK: Color32 = Color32::from_rgb(70, 170, 90);
const MISSING: Color32 = Color32::from_rgb(210, 70, 70);
const UNKNOWN: Color32 = Color32::from_rgb(200, 160, 60);

/// Page 1: the checklist, with official links only.
pub fn setup(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Setup");
    ui.label("Green means the launcher found the item; grey-yellow means it cannot tell.");
    ui.add_space(6.0);
    for item in &app.setup {
        ui.group(|ui| {
            ui.label(RichText::new(item.title).strong());
            let (text, color) = match &item.state {
                State::Ok(text) => (format!("OK: {text}"), OK),
                State::Missing(text) => (format!("Missing: {text}"), MISSING),
                State::Unknown(text) => (format!("Unknown: {text}"), UNKNOWN),
            };
            ui.colored_label(color, text);
            ui.label(format!("Needed for: {}", item.needed_for));
            if let Some((label, url)) = item.link {
                ui.hyperlink_to(label, url);
            }
        });
    }
    ui.add_space(6.0);
    ui.label(format!(
        "Host: {:?}. GPU: {} ({}).",
        app.host,
        app.gpu.label(),
        app.adapter_name.as_deref().unwrap_or("no adapter found")
    ));
    ui.label(format!(
        "Profile: {} ({:?}).",
        app.profile().name,
        app.profile().status
    ));
    ui.add_space(6.0);
    if ui.button("Check again").clicked() {
        app.refresh_setup();
    }
}

/// Page 2: the game folder, its check, and the manifest.
pub fn files(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Game folder");
    ui.label("The folder holding game_scarlett_master.exe, outside this repository. Only its own files are read.");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut app.settings.game_dir);
        if ui
            .add_enabled(!app.is_busy(), egui::Button::new("Check folder"))
            .clicked()
        {
            app.start_verify();
        }
    });
    ui.add_space(6.0);
    match &app.verify {
        Verify::NotRun => {
            ui.label("Not checked yet.");
        }
        Verify::Running => {
            ui.label("Checking (hashing large archives takes a while)...");
        }
        Verify::Failed(error) => {
            ui.colored_label(MISSING, format!("Cannot check: {error}"));
        }
        Verify::Done(report) => {
            if report.is_ok() {
                ui.colored_label(OK, "Every required file is present and whole.");
            }
            for (name, problem) in &report.problems {
                let text = match problem {
                    Problem::Missing => "missing".to_string(),
                    Problem::Corrupted(reason) => format!("corrupted: {reason}"),
                    Problem::Unreadable(reason) => format!("unreadable: {reason}"),
                };
                ui.colored_label(MISSING, format!("{name}: {text}"));
            }
            ui.add_space(6.0);
            ui.label("Required files:");
            for (name, reason) in verify::required_files() {
                ui.label(format!("  {name}: {reason}"));
            }
            ui.add_space(6.0);
            ui.collapsing("SHA-256 manifest", |ui| {
                let mut text = report.manifest_text();
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(6)
                        .font(egui::TextStyle::Monospace),
                );
                if ui.button("Copy manifest").clicked() {
                    ui.ctx().copy_text(report.manifest_text());
                }
            });
        }
    }
}

/// Page 3: shader preloading (VIctory's translate into the cache), with its progress.
pub fn shaders(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Shader preloading");
    ui.label("Translates shaders.rpf and the game's two DLLs into VIctory's cache, so the first launch does not wait. Several minutes the first time.");
    ui.label("The cache depends on the settings: change them, and the cache is filled again.");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Worker threads (0 = every core):");
        ui.add(egui::DragValue::new(&mut app.settings.jobs).range(0..=64));
    });
    let ready = matches!(&app.verify, Verify::Done(report) if report.is_ok());
    if !ready {
        ui.colored_label(UNKNOWN, "Check the game folder first (page 2).");
    }
    let enabled = ready && !app.is_busy();
    if ui
        .add_enabled(enabled, egui::Button::new("Preload shaders"))
        .clicked()
    {
        app.start(true);
    }
    if let Some(progress) = app.progress() {
        ui.add(egui::ProgressBar::new(progress.fraction()).text(format!(
            "{} of {} {} ({}%)",
            progress.done, progress.total, progress.what, progress.percent
        )));
    }
    ui::log_view(app, ui);
}

/// Page 4: the launch, the profile, the XG library, the advanced panel and the cache.
pub fn play(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Play");
    let profile = app.profile().clone();
    ui.label(RichText::new(format!("Profile: {}", profile.name)).strong());
    ui.label(format!("Status: {:?}. {}", profile.status, profile.note));
    if !profile.pending.is_empty() {
        ui.colored_label(
            UNKNOWN,
            format!(
                "Pending the maintainer's confirmation (set to 0 in this profile): {}",
                profile.pending.join(", ")
            ),
        );
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("XG library (your own xg_xs.dll, optional):");
        ui.text_edit_singleline(&mut app.settings.xg_dll);
    });
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.settings.xg_mode, XgMode::Real, "real");
        ui.radio_value(&mut app.settings.xg_mode, XgMode::Compare, "compare");
    });
    ui.add_space(6.0);
    let ready = matches!(&app.verify, Verify::Done(report) if report.is_ok());
    if !ready {
        ui.colored_label(UNKNOWN, "Check the game folder first (page 2).");
    }
    if ui
        .add_enabled(ready && !app.is_busy(), egui::Button::new("Launch game"))
        .clicked()
    {
        app.start(false);
    }
    ui.add_space(6.0);
    advanced(app, ui);
    ui.add_space(6.0);
    ui.label(format!("Saves and data: {}", app.data_dir_label()));
    ui.horizontal(|ui| {
        ui.label(format!(
            "Shader cache: {}",
            app.cache_dir
                .as_ref()
                .map_or("unknown".to_string(), |d| d.display().to_string())
        ));
        if ui
            .add_enabled(!app.is_busy(), egui::Button::new("Clean shader cache"))
            .clicked()
        {
            app.confirm_clean = true;
        }
    });
    ui::log_view(app, ui);
}

/// The advanced panel: overrides, the guard, and the environment the launch will use.
fn advanced(app: &mut App, ui: &mut egui::Ui) {
    ui.collapsing("Advanced", |ui| {
        ui.label("Overrides, one KEY=VALUE per line (VICTORY_ or VKD3D_ names). They win over the profile.");
        ui.add(
            egui::TextEdit::multiline(&mut app.settings.overrides)
                .desired_rows(5)
                .font(egui::TextStyle::Monospace),
        );
        ui.checkbox(
            &mut app.settings.skip_process_check,
            "Skip the running-game check (only if you are sure no game runs)",
        );
        let profile = app.profile().clone();
        match crate::env::parse_overrides(&app.settings.overrides) {
            Ok(pairs) => {
                let changed = crate::env::overridden_keys(&profile, &pairs);
                if !changed.is_empty() {
                    ui.label(format!("Overrides profile values: {}", changed.join(", ")));
                }
            }
            Err(error) => {
                ui.colored_label(MISSING, error.to_string());
            }
        }
        if let Ok(plan) = session::make_plan(&app.settings, &app.repo_or_empty(), &profile) {
            ui.collapsing("Environment of the launch", |ui| {
                for line in crate::env::describe(&plan.env) {
                    ui.monospace(line);
                }
            });
        }
    });
}

/// The confirmation window of the cache clean.
pub fn confirm_clean(app: &mut App, ctx: &egui::Context) {
    if !app.confirm_clean {
        return;
    }
    let target = app
        .cache_dir
        .as_ref()
        .map_or("unknown folder".to_string(), |d| d.display().to_string());
    let mut close = false;
    egui::Window::new("Clean the shader cache?")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("This deletes {target}."));
            ui.label(
                "The next launches translate the shaders again (minutes). Saves are not touched.",
            );
            ui.horizontal(|ui| {
                if ui.button("Delete the cache").clicked() {
                    app.clean_cache();
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close {
        app.confirm_clean = false;
    }
}
