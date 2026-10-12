//! Window layout: top bar, game list on the left, detail pane (Overview, Mods, Patches) on the right.

use egui::{Color32, RichText};

use super::patches::{self, show};
use super::{Action, App, Confirm, Tab};
use crate::config::Game;
use crate::mods::ModState;
use crate::status::{ModsInfo, Preflight};

const ORANGE: Color32 = Color32::from_rgb(255, 176, 32);

fn status_colour(status: &str) -> Color32 {
    match status {
        "ok" => Color32::LIGHT_GREEN,
        "warning" => ORANGE,
        _ => Color32::LIGHT_RED,
    }
}

fn renpy7_badge() -> RichText {
    RichText::new(" Ren'Py 7: needs the compatibility module ")
        .color(Color32::BLACK)
        .background_color(ORANGE)
}

impl App {
    pub(super) fn draw(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();

        // Files and folders dropped on the window are mods for the selected game.
        let dropped: Vec<std::path::PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if let Some(first) = dropped.first() {
            if dropped.len() > 1 {
                self.message =
                    "Only the first dropped item is installed; drop the others one at a time."
                        .into();
            }
            actions.push(Action::PlanInstall(first.clone()));
        }
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Universal Ren'Py Player");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Settings").clicked() {
                        actions.push(Action::OpenSettings);
                    }
                    if ui.button("Rescan").clicked() {
                        actions.push(Action::Rescan);
                    }
                    if ui.button("Add folder...").clicked() {
                        actions.push(Action::AddFolder);
                    }
                    if ui.button("Import archive...").clicked() {
                        actions.push(Action::ImportArchive);
                    }
                });
            });
            ui.label(
                RichText::new(format!(
                    "{} games in {} folders. Data: {}",
                    self.library.games.len(),
                    self.library.folders.len(),
                    self.data.display()
                ))
                .small()
                .weak(),
            );
            if !self.message.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.message).color(Color32::LIGHT_BLUE));
                    if ui.small_button("x").clicked() {
                        self.message.clear();
                    }
                });
            }
            ui.add_space(4.0);
        });

        egui::Panel::left("games")
            .default_size(380.0)
            .min_size(260.0)
            .resizable(true)
            .show(ui, |ui| self.game_list(ui, &mut actions));

        egui::CentralPanel::default().show(ui, |ui| self.detail(ui, &mut actions));

        self.dialogs(ui.ctx(), &mut actions);

        if hovering {
            let screen = ui.ctx().content_rect();
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop-hint"),
            ));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                if self.selected.is_some() {
                    "Drop a mod folder or archive to install it for the selected game"
                } else {
                    "Select a game first, then drop the mod"
                },
                egui::FontId::proportional(20.0),
                Color32::WHITE,
            );
        }
        actions
    }

    fn game_list(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if self.library.games.is_empty() {
            ui.add_space(24.0);
            ui.label("No games yet. Press \"Add folder...\" and choose a folder that holds Ren'Py games.");
        }
        let games = self.library.games.clone();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for g in &games {
                    let (pre, mods) = self.status.get(&g.key).cloned().unwrap_or_default();
                    let selected = self.selected.as_ref() == Some(&g.key);
                    let running = self.running.iter().any(|(k, _)| *k == g.key);
                    let streaming = self.streams.iter().any(|(k, _)| *k == g.key);
                    let icon = self.icon(g);
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let row = ui
                        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                match &icon {
                                    Some(t) => {
                                        ui.add(
                                            egui::Image::new(t)
                                                .fit_to_exact_size(egui::vec2(44.0, 44.0)),
                                        );
                                    }
                                    None => {
                                        ui.allocate_space(egui::vec2(44.0, 44.0));
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&g.name).strong())
                                            .truncate(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        match &g.engine {
                                            Some(v) => ui.label(
                                                RichText::new(format!("Ren'Py {v}")).small(),
                                            ),
                                            None => ui.label(
                                                RichText::new("version unknown").small().weak(),
                                            ),
                                        };
                                        if g.renpy7 {
                                            ui.label(
                                                RichText::new(" 7 ")
                                                    .small()
                                                    .color(Color32::BLACK)
                                                    .background_color(ORANGE),
                                            );
                                        }
                                        if mods.installed > 0 {
                                            ui.label(
                                                RichText::new(format!(
                                                    "mods {}/{}",
                                                    mods.enabled, mods.installed
                                                ))
                                                .small(),
                                            );
                                        }
                                        if let Some(p) = &pre {
                                            ui.label(
                                                RichText::new(&p.status)
                                                    .small()
                                                    .color(status_colour(&p.status)),
                                            );
                                        }
                                        if streaming {
                                            ui.label(
                                                RichText::new("streaming")
                                                    .small()
                                                    .color(Color32::LIGHT_GREEN),
                                            );
                                        } else if running {
                                            ui.label(
                                                RichText::new("running")
                                                    .small()
                                                    .color(Color32::LIGHT_GREEN),
                                            );
                                        }
                                    });
                                });
                            });
                        })
                        .response;
                    let fill = if selected {
                        Some(ui.visuals().selection.bg_fill)
                    } else if row.hovered() {
                        Some(ui.visuals().widgets.hovered.weak_bg_fill)
                    } else {
                        None
                    };
                    if let Some(fill) = fill {
                        ui.painter()
                            .set(bg, egui::epaint::RectShape::filled(row.rect, 4.0, fill));
                    }
                    if row.clicked() {
                        actions.push(Action::Select(g.key.clone()));
                    }
                }
            });
    }

    fn detail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(i) = self.selected_index() else {
            ui.add_space(24.0);
            ui.label("Select a game on the left.");
            return;
        };
        let g = self.library.games[i].clone();
        ui.horizontal(|ui| {
            if let Some(t) = self.icon(&g) {
                ui.add(egui::Image::new(&t).fit_to_exact_size(egui::vec2(40.0, 40.0)));
            }
            ui.heading(&g.name);
        });
        ui.horizontal(|ui| {
            for (t, label) in [
                (Tab::Overview, "Overview"),
                (Tab::Mods, "Mods"),
                (Tab::Patches, "Patches"),
            ] {
                ui.selectable_value(&mut self.tab, t, label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| match self.tab {
                Tab::Overview => self.overview(ui, i, &g, actions),
                Tab::Mods => self.mods_tab(ui, actions),
                Tab::Patches => self.patches_tab(ui, &g, actions),
            });
    }

    fn overview(&mut self, ui: &mut egui::Ui, i: usize, g: &Game, actions: &mut Vec<Action>) {
        let (pre, mods): (Option<Preflight>, ModsInfo) =
            self.status.get(&g.key).cloned().unwrap_or_default();
        let streaming = self.streams.iter().any(|(k, _)| *k == g.key);
        let running = streaming || self.running.iter().any(|(k, _)| *k == g.key);
        let urls: Vec<String> = self
            .streams
            .iter()
            .find(|(k, _)| *k == g.key)
            .map(|(_, s)| s.urls.clone())
            .unwrap_or_default();

        egui::Grid::new("overview")
            .num_columns(2)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label("Path");
                ui.add(egui::Label::new(g.path.display().to_string()).wrap());
                ui.end_row();
                ui.label("Ren'Py");
                ui.horizontal_wrapped(|ui| {
                    match &g.engine {
                        Some(v) => ui.label(v),
                        None => ui.label(RichText::new("version unknown").weak()),
                    };
                    if g.renpy7 {
                        ui.label(renpy7_badge());
                    }
                });
                ui.end_row();
                ui.label("Pre-flight");
                match &pre {
                    None => ui.label(RichText::new("not run yet").weak()),
                    Some(p) => ui.label(RichText::new(&p.status).color(status_colour(&p.status))),
                };
                ui.end_row();
                ui.label("Mods");
                ui.label(if mods.installed == 0 {
                    "none installed".to_string()
                } else {
                    format!("{} of {} enabled", mods.enabled, mods.installed)
                });
                ui.end_row();
            });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let label = if running { "Running" } else { "Play" };
            if ui.add_enabled(!running, egui::Button::new(label)).clicked() {
                actions.push(Action::Play(i));
            }
            if streaming {
                if ui.button("Stop stream").clicked() {
                    actions.push(Action::StopStream(i));
                }
            } else if ui
                .add_enabled(!running, egui::Button::new("Stream"))
                .clicked()
            {
                actions.push(Action::Stream(i));
            }
            if ui.button("Report").clicked() {
                actions.push(Action::Report(i));
            }
            if ui.button("Open folder").clicked() {
                actions.push(Action::OpenFolder(i));
            }
        });
        if streaming {
            ui.add_space(6.0);
            if urls.is_empty() {
                ui.label(RichText::new("Streaming: starting...").weak());
            }
            for u in &urls {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Open in a browser:").color(Color32::LIGHT_GREEN));
                    ui.monospace(u);
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(u.clone());
                    }
                });
            }
        }
        ui.add_space(24.0);
        ui.separator();
        if ui
            .button(RichText::new("Remove from library...").color(Color32::LIGHT_RED))
            .on_hover_text("Removes only the library entry. The game's files are not touched.")
            .clicked()
        {
            self.confirm = Some(Confirm::Game {
                key: g.key.clone(),
                name: g.name.clone(),
            });
        }
    }

    fn mods_tab(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui.button("Install mod from file...").clicked() {
                actions.push(Action::PickMod(false));
            }
            if ui.button("Install mod from folder...").clicked() {
                actions.push(Action::PickMod(true));
            }
            ui.label(RichText::new("or drop a file or folder on the window").weak());
        });
        ui.add_space(6.0);
        if !self.mods_error.is_empty() {
            ui.label(RichText::new(&self.mods_error).color(Color32::LIGHT_RED));
        }
        if self.mods.is_empty() {
            ui.label(RichText::new("No mods installed for this game.").weak());
            return;
        }
        ui.label(
            RichText::new("Top of the list wins when two mods change the same file.")
                .small()
                .weak(),
        );
        ui.add_space(4.0);
        // Highest priority first: that is the end of `order.txt`.
        let enabled: Vec<_> = self.mods.iter().filter(|m| m.order.is_some()).collect();
        let (first, last) = (
            enabled.last().map(|m| m.name.clone()),
            enabled.first().map(|m| m.name.clone()),
        );
        let shown = self
            .mods
            .iter()
            .filter(|m| m.order.is_some())
            .rev()
            .chain(self.mods.iter().filter(|m| m.order.is_none()));
        for m in shown {
            ui.horizontal(|ui| {
                let mut on = m.state != ModState::Disabled;
                if ui.checkbox(&mut on, "").changed() {
                    actions.push(Action::ModEnable(m.name.clone(), on));
                }
                ui.add(egui::Label::new(RichText::new(&m.name).strong()).truncate());
                match m.state {
                    ModState::Missing => {
                        ui.label(RichText::new("folder missing").color(Color32::LIGHT_RED));
                    }
                    ModState::Disabled => {
                        ui.label(RichText::new("disabled").weak());
                    }
                    ModState::Enabled => {}
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Remove").clicked() {
                        self.confirm = Some(Confirm::Mod(m.name.clone()));
                    }
                    if m.order.is_some() {
                        let down = ui
                            .add_enabled(last.as_ref() != Some(&m.name), egui::Button::new("Down"));
                        if down.clicked() {
                            actions.push(Action::ModMove(m.name.clone(), -1));
                        }
                        let up = ui
                            .add_enabled(first.as_ref() != Some(&m.name), egui::Button::new("Up"));
                        if up.clicked() {
                            actions.push(Action::ModMove(m.name.clone(), 1));
                        }
                    }
                });
            });
            ui.separator();
        }
    }

    fn patches_tab(&mut self, ui: &mut egui::Ui, g: &Game, actions: &mut Vec<Action>) {
        let requested = self.patches.get(&g.key).is_some_and(|v| v.requested);
        if !requested {
            actions.push(Action::Patches("list"));
        }
        let Some(view) = self.patches.get(&g.key) else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the patch library...");
            });
            return;
        };
        let busy = view.busy;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(busy.is_none(), |ui| {
                if ui.button("List").clicked() {
                    actions.push(Action::Patches("list"));
                }
                if ui.button("Validate").clicked() {
                    actions.push(Action::Patches("validate"));
                }
                if ui
                    .button("Apply test")
                    .on_hover_text(
                        "Starts the engine without a window and checks that every patch matches",
                    )
                    .clicked()
                {
                    actions.push(Action::Patches("apply-test"));
                }
            });
            if let Some(sub) = busy {
                ui.spinner();
                ui.label(match sub {
                    "apply-test" => "Applying the patches in a headless engine...",
                    "validate" => "Validating...",
                    _ => "Listing...",
                });
            }
        });
        if !view.error.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(&view.error)
                    .monospace()
                    .color(Color32::LIGHT_RED),
            );
        }
        if !view.note.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(&view.note).color(Color32::LIGHT_GREEN));
        }
        ui.add_space(6.0);

        if let Some((kind, r)) = &view.list {
            let j = &r.json;
            let arr = |k: &str| j[k].as_array().cloned().unwrap_or_default();
            ui.separator();
            let heading = if *kind == "validate" {
                "Validation"
            } else {
                "Patch library"
            };
            ui.strong(heading);
            let ok = j["ok"].as_bool().unwrap_or(false);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{} patches in {} files",
                    arr("patches").len(),
                    arr("files").len()
                ));
                if ok {
                    ui.label(RichText::new("ok").color(Color32::LIGHT_GREEN));
                } else {
                    ui.label(RichText::new("has errors").color(Color32::LIGHT_RED));
                }
                match j["fingerprint"].as_str() {
                    Some(f) => ui.label(RichText::new(format!("build {f}")).small().weak()),
                    None => ui.label(
                        RichText::new("build unknown: the game has not started yet")
                            .small()
                            .weak(),
                    ),
                };
            });
            for e in arr("errors") {
                ui.label(
                    RichText::new(show(&e))
                        .monospace()
                        .color(Color32::LIGHT_RED),
                );
            }
            let patches = arr("patches");
            if !patches.is_empty() {
                egui::Grid::new("patch-list")
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for p in &patches {
                            ui.monospace(format!("{}:{}", show(&p["file"]), show(&p["line"])));
                            let origin = show(&p["origin"]);
                            let stem = patches::patch_id(&origin).unwrap_or(origin);
                            ui.label(format!("{stem} #{}", show(&p["index"])));
                            let hash = show(&p["original_hash"]);
                            ui.label(
                                RichText::new(format!("sha1 {}", hash.get(..10).unwrap_or(&hash)))
                                    .small()
                                    .weak(),
                            );
                            ui.end_row();
                        }
                    });
            }
            let inactive = arr("inactive");
            if !inactive.is_empty() {
                ui.add_space(6.0);
                ui.strong("Not active");
                egui::Grid::new("patch-inactive")
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for p in &inactive {
                            let path = show(&p["path"]);
                            let state = show(&p["state"]);
                            let id = patches::patch_id(&path).unwrap_or_else(|| path.clone());
                            ui.label(&id);
                            ui.label(RichText::new(&state).color(ORANGE));
                            if state == "proposed"
                                && ui
                                    .add_enabled(busy.is_none(), egui::Button::new("Accept"))
                                    .on_hover_text("Marks this verified proposal as accepted so that it applies")
                                    .clicked()
                            {
                                actions.push(Action::AcceptPatch(id));
                            }
                            ui.end_row();
                        }
                    });
            }
        }

        if let Some(r) = &view.apply {
            let j = &r.json;
            let arr = |k: &str| j[k].as_array().cloned().unwrap_or_default();
            ui.add_space(10.0);
            ui.separator();
            ui.strong("Apply test");
            let (applied, unmatched, errors) = (arr("applied"), arr("unmatched"), arr("errors"));
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} applied", applied.len()));
                let c = if unmatched.is_empty() {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::LIGHT_RED
                };
                ui.label(RichText::new(format!("{} unmatched", unmatched.len())).color(c));
                if !errors.is_empty() {
                    ui.label(
                        RichText::new(format!("{} errors", errors.len())).color(Color32::LIGHT_RED),
                    );
                }
            });
            for e in &errors {
                ui.label(RichText::new(show(e)).monospace().color(Color32::LIGHT_RED));
            }
            if !unmatched.is_empty() {
                ui.label("Unmatched");
                for (n, u) in unmatched.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{}:{}", show(&u["file"]), show(&u["line"])));
                        ui.label(show(&u["patch"]));
                    });
                    ui.indent(n, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(show(&u["reason"])).color(Color32::LIGHT_RED),
                            )
                            .wrap(),
                        );
                    });
                    ui.add_space(4.0);
                }
            }
            if !applied.is_empty() {
                ui.label("Applied");
                egui::Grid::new("patch-applied")
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for a in &applied {
                            ui.monospace(format!("{}:{}", show(&a["file"]), show(&a["line"])));
                            ui.label(show(&a["patch"]));
                            ui.label(RichText::new(show(&a["node"])).small().weak());
                            ui.end_row();
                        }
                    });
            }
        }
    }
}
