//! Modal dialogs: Settings, confirmations, and the mod install steps.

use egui::{Color32, Id, Modal, RichText};

use super::{Action, App, Confirm, Install};

const RENDERERS: [(&str, &str); 3] = [
    ("", "Default"),
    ("wgpu", "wgpu (GPU)"),
    ("sw", "sw (software)"),
];

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

impl App {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.settings_dialog(ctx, actions);
        self.install_dialog(ctx, actions);
        self.confirm_dialog(ctx, actions);
    }

    fn settings_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(mut d) = self.settings.take() else {
            return;
        };
        let mut close = false;
        let resp = Modal::new(Id::new("settings")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading("Settings");
            ui.add_space(8.0);
            ui.strong("Scanned folders");
            if self.library.folders.is_empty() {
                ui.label(RichText::new("None. Add a folder that holds Ren'Py games.").weak());
            }
            let folders = self.library.folders.clone();
            for f in &folders {
                ui.horizontal(|ui| {
                    if ui.small_button("Remove").clicked() {
                        self.confirm = Some(Confirm::Folder(f.clone()));
                    }
                    ui.add(egui::Label::new(f.display().to_string()).truncate());
                });
            }
            if ui.button("Add folder...").clicked() {
                actions.push(Action::AddFolder);
            }
            ui.add_space(10.0);
            ui.strong("Renderer");
            ui.horizontal(|ui| {
                ui.label("Draw backend");
                let shown = RENDERERS
                    .iter()
                    .find(|(v, _)| *v == d.renderer)
                    .map_or(d.renderer.as_str(), |(_, l)| l);
                egui::ComboBox::from_id_salt("renderer")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (v, label) in RENDERERS {
                            ui.selectable_value(&mut d.renderer, v.to_string(), label);
                        }
                    });
                ui.label(
                    RichText::new("games started from here use it (`--renderer`)")
                        .small()
                        .weak(),
                );
            });
            ui.add_space(10.0);
            ui.strong("Stream defaults");
            egui::Grid::new("stream-defaults")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Port (0 = pick a free one)");
                    ui.add(egui::TextEdit::singleline(&mut d.port).desired_width(80.0));
                    ui.end_row();
                    ui.label("Size");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut d.width).desired_width(60.0));
                        ui.label("x");
                        ui.add(egui::TextEdit::singleline(&mut d.height).desired_width(60.0));
                    });
                    ui.end_row();
                    ui.label("Frames per second");
                    ui.add(egui::TextEdit::singleline(&mut d.fps).desired_width(80.0));
                    ui.end_row();
                    ui.label("Bitrate (kbps)");
                    ui.add(egui::TextEdit::singleline(&mut d.kbps).desired_width(80.0));
                    ui.end_row();
                });
            if !d.error.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(&d.error).color(Color32::LIGHT_RED));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    actions.push(Action::SaveSettings);
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
        // A confirm dialog on top gets the Escape key first.
        if !(close || (resp.should_close() && self.confirm.is_none())) {
            self.settings = Some(d);
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(c) = &self.confirm else { return };
        let (title, body, button) = match c {
            Confirm::Game { name, .. } => (
                "Remove from library".to_string(),
                format!(
                    "Remove {name} from the library?\n\nOnly the library entry goes. The game's files are not touched, and a rescan will not bring it back until you add its folder again."
                ),
                "Remove",
            ),
            Confirm::Mod(name) => (
                "Remove mod".to_string(),
                format!(
                    "Remove the mod {name}?\n\nIts files in the player's data folder are deleted."
                ),
                "Remove",
            ),
            Confirm::Folder(p) => (
                "Remove scanned folder".to_string(),
                format!(
                    "Stop scanning {}?\n\nGames found only through this folder leave the library. Nothing on disk is deleted.",
                    p.display()
                ),
                "Remove",
            ),
        };
        let mut yes = false;
        let mut no = false;
        let resp = Modal::new(Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(button).color(Color32::LIGHT_RED))
                    .clicked()
                {
                    yes = true;
                }
                if ui.button("Cancel").clicked() {
                    no = true;
                }
            });
        });
        if yes {
            match self.confirm.take() {
                Some(Confirm::Game { key, .. }) => actions.push(Action::RemoveGame(key)),
                Some(Confirm::Mod(name)) => actions.push(Action::ModRemove(name)),
                Some(Confirm::Folder(p)) => actions.push(Action::RemoveFolder(p)),
                None => {}
            }
        } else if no || resp.should_close() {
            self.confirm = None;
        }
    }

    fn install_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(mut inst) = self.install.take() else {
            return;
        };
        let mut close = false;
        let resp = Modal::new(Id::new("install")).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.heading("Install mod");
            ui.add_space(6.0);
            match &mut inst {
                Install::Planning(path) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Reading {}...", path.display()));
                    });
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                }
                Install::Installing(name) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Installing {name}..."));
                    });
                }
                Install::Failed(e) => {
                    ui.label(RichText::new(&*e).color(Color32::LIGHT_RED));
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
                Install::Confirm { plan, name, error } => {
                    egui::Grid::new("install-plan")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Mod name");
                            ui.add(egui::TextEdit::singleline(name).desired_width(320.0));
                            ui.end_row();
                            ui.label("Source");
                            ui.add(egui::Label::new(plan.source.display().to_string()).truncate());
                            ui.end_row();
                            ui.label("Layout");
                            ui.label(plan.layout.describe());
                            ui.end_row();
                            ui.label("Files");
                            ui.label(format!(
                                "{} files, {} unpacked",
                                plan.files.len(),
                                human_bytes(plan.unpacked_bytes)
                            ));
                            ui.end_row();
                        });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Installed into {}", plan.mod_dir().display()))
                            .small()
                            .weak(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for f in &plan.files {
                                ui.add(
                                    egui::Label::new(RichText::new(f).monospace().small())
                                        .truncate(),
                                );
                            }
                        });
                    if !error.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&*error).color(Color32::LIGHT_RED));
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Install").clicked() {
                            actions.push(Action::CommitInstall);
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        let installing = matches!(inst, Install::Installing(_));
        if !(close || (resp.should_close() && !installing)) {
            self.install = Some(inst);
        }
    }
}
