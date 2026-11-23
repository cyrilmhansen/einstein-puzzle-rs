use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Layout, RichText, ScrollArea, Ui, Vec2};
use einstein_core::RulePlacement;
use einstein_core::session::{Command, Effect, HintView, Session, Snapshot, Status};

use crate::navigation::Navigation;
use crate::{board, theme::Classic};

pub(crate) struct App {
    session: Session,
    next_seed: u64,
    savedir: PathBuf,
    slot: u8,
    zoom: f32,
    now: Instant,
    note: String,
    help: bool,
    help_focus_pending: bool,
    restore_rules_focus: bool,
    nav: Navigation,
    classic: bool,
    historic: Option<Classic>,
}

impl App {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        seed: u64,
        savedir: PathBuf,
        classic: bool,
    ) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(13, 22, 39);
        visuals.faint_bg_color = Color32::from_rgb(19, 31, 52);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 47, 73);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 71, 101);
        visuals.widgets.active.bg_fill = Color32::from_rgb(53, 83, 117);
        visuals.selection.stroke.color = Color32::from_rgb(129, 218, 255);
        cc.egui_ctx.set_visuals(visuals);
        let mut app = Self {
            session: Session::new(),
            next_seed: seed,
            savedir,
            slot: 0,
            zoom: 1.0,
            now: Instant::now(),
            note: String::new(),
            help: false,
            help_focus_pending: false,
            restore_rules_focus: false,
            nav: Navigation::default(),
            classic,
            historic: None,
        };
        app.new_puzzle();
        app
    }

    fn new_puzzle(&mut self) {
        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(1);
        self.apply(Command::New { seed });
    }

    fn apply(&mut self, c: Command) {
        match self.session.apply(c) {
            Ok(Effect::Failed | Effect::Won) => self.note.clear(),
            Ok(Effect::Changed) => {
                if matches!(c, Command::New { .. } | Command::Restart | Command::Resume) {
                    self.now = Instant::now();
                }
                if matches!(c, Command::New { .. } | Command::Restart | Command::End) {
                    self.note.clear();
                }
            }
            Ok(Effect::Unchanged) => {}
            Err(err) => self.note = err.to_string(),
        }
    }

    fn save(&mut self) {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        let result = builder
            .create(&self.savedir)
            .map_err(einstein_storage::Error::from)
            .and_then(|()| {
                einstein_storage::save(
                    &self.session,
                    self.savedir.join(format!("slot-{}.epz", self.slot)),
                )
            });
        self.note = match result {
            Ok(()) => format!("Partie sauvegardée dans l’emplacement {}.", self.slot),
            Err(e) => format!("Sauvegarde impossible : {e}"),
        };
    }

    fn load(&mut self) {
        self.note = match einstein_storage::load_into(
            &mut self.session,
            self.savedir.join(format!("slot-{}.epz", self.slot)),
        ) {
            Ok(()) => {
                self.now = Instant::now();
                format!("Emplacement {} chargé.", self.slot)
            }
            Err(e) => format!("Chargement impossible : {e}"),
        };
    }

    fn toolbar(&mut self, ui: &mut Ui, state: Option<&Snapshot>) {
        let status = state.map(|s| s.status);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Nouvelle grille").clicked() {
                self.new_puzzle();
            }
            if ui
                .add_enabled(status.is_some(), egui::Button::new("Recommencer"))
                .clicked()
            {
                self.apply(Command::Restart);
            }
            let paused = status == Some(Status::Paused);
            if ui
                .add_enabled(
                    paused || status == Some(Status::Playing),
                    egui::Button::new(if paused { "Reprendre" } else { "Pause" }),
                )
                .clicked()
            {
                self.apply(if paused {
                    Command::Resume
                } else {
                    Command::Pause
                });
            }
            if ui
                .add_enabled(
                    status == Some(Status::Playing),
                    egui::Button::new("Basculer les indices"),
                )
                .clicked()
            {
                self.apply(Command::SwitchHints);
            }
            let rules = ui.button("Règles");
            if self.restore_rules_focus && rules.enabled() {
                self.restore_rules_focus = false;
                rules.request_focus();
            }
            if rules.clicked() && !self.help {
                self.help = true;
                self.help_focus_pending = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Emplacement");
            let slot_control = egui::ComboBox::from_id_salt("save-slot")
                .selected_text(self.slot.to_string())
                .width(48.0)
                .show_ui(ui, |ui| {
                    for slot in 0..10 {
                        ui.selectable_value(&mut self.slot, slot, format!("Emplacement {slot}"));
                    }
                });
            slot_control.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::ComboBox,
                    true,
                    format!("Emplacement {}", self.slot),
                )
            });
            if ui
                .add_enabled(status.is_some(), egui::Button::new("Sauvegarder"))
                .clicked()
            {
                self.save();
            }
            if ui.button("Charger").clicked() {
                self.load();
            }
            ui.separator();
            ui.label("Zoom du plateau");
            if ui
                .add_enabled(self.zoom > 0.85, egui::Button::new("Réduire"))
                .clicked()
            {
                self.zoom = (self.zoom - 0.15).max(0.85);
            }
            if ui
                .add_enabled(self.zoom < 1.9, egui::Button::new("Agrandir"))
                .clicked()
            {
                self.zoom = (self.zoom + 0.15).min(1.9);
            }
            ui.label(format!("{:.0} %", self.zoom * 100.0));
            ui.separator();
            ui.checkbox(&mut self.classic, "Thème classique");
        });
    }

    fn show_help(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        let pending = std::mem::take(&mut self.help_focus_pending);
        let modal = egui::Modal::new(egui::Id::unique("einstein-rules-modal")).show(ctx, |ui| {
            ui.set_max_width(445.0);
            ui.heading("Règles et commandes");
            ui.label("Chaque catégorie forme une permutation. Utilisez les indices pour déduire l’emplacement des six cartes.");
            ui.label("Sélection : clic gauche ou Entrée. Exclusion : clic droit ou Suppr sur le candidat. Tab / Maj+Tab : contrôle suivant / précédent.");
            ui.label("Flèches : déplacer le focus. F6 / Maj+F6 : changer de zone. Ctrl+Début / Ctrl+Fin : premier / dernier élément.");
            ui.label("Cliquez un indice pour le masquer. Basculez le panneau, puis cliquez cet indice pour le rétablir.");
            let close = ui.button("Fermer les règles");
            if pending {
                close.request_focus();
                ui.ctx().request_repaint();
            }
            close.clicked()
        });
        if modal.inner || modal.should_close() {
            self.help = false;
            self.restore_rules_focus = true;
            ctx.request_repaint();
        }
    }

    fn play(&mut self, ui: &mut Ui, state: &Snapshot) {
        let total = ui.available_size();
        let right = (total.x * 0.31).clamp(174.0, 310.0);
        let left = (total.x - right - 14.0).max(270.0);
        let active = state.status == Status::Playing;
        let mut command = None;
        if self.help {
            self.nav.clear();
        }
        self.nav.begin(ui);
        let skin = board::Skin {
            zoom: self.zoom,
            classic: self.historic.as_ref().filter(|_| self.classic),
        };
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left, total.y),
                Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    ui.heading("Grille des possibilités");
                    let lower_height = 92.0;
                    let grid_height = (ui.available_height() - lower_height - 36.0).max(100.0);
                    ScrollArea::both()
                        .id_salt("puzzle-grid")
                        .max_width(left)
                        .max_height(grid_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            board::grid(ui, state, skin, active, &mut command, &mut self.nav)
                        });
                    ui.separator();
                    ui.heading("Indices de colonne");
                    ScrollArea::horizontal()
                        .id_salt("vertical-clues")
                        .max_width(left)
                        .max_height(lower_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            board::clues(
                                ui,
                                state,
                                RulePlacement::Vertical,
                                100.0 * self.zoom,
                                skin,
                                active,
                                &mut command,
                                &mut self.nav,
                            );
                        });
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                Vec2::new(right, total.y),
                Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(right);
                    ui.heading("Indices de relation");
                    ui.label(match state.hint_view {
                        HintView::Active => "Indices actifs",
                        HintView::Dismissed => "Indices masqués",
                    });
                    ScrollArea::vertical()
                        .id_salt("horizontal-clues")
                        .max_height((ui.available_height() - 3.0).max(90.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            board::clues(
                                ui,
                                state,
                                RulePlacement::Horizontal,
                                (right - 18.0).max(140.0),
                                skin,
                                active,
                                &mut command,
                                &mut self.nav,
                            );
                        });
                },
            );
        });
        self.nav.end();
        if let Some(command) = command {
            self.apply(command);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let millis = self.now.elapsed().as_millis().min(u64::MAX.into()) as u64;
        if millis != 0 {
            self.now += Duration::from_millis(millis);
            let _ = self.session.apply(Command::AdvanceClock { millis });
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let state = self.session.snapshot();
        if self.classic && self.historic.is_none() {
            self.historic = Some(Classic::load(ui.ctx()));
        }
        egui::Frame::central_panel(ui.style())
            .inner_margin(14)
            .show(ui, |ui| {
                if self.classic
                    && let Some(theme) = &self.historic
                {
                    theme.wallpaper(ui);
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Einstein Puzzle")
                            .size(28.0)
                            .strong()
                            .color(Color32::from_rgb(228, 240, 255)),
                    );
                    ui.label(
                        RichText::new("LOGIQUE  ·  6 × 6")
                            .strong()
                            .color(Color32::from_rgb(124, 199, 232)),
                    );
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        let seconds = state.as_ref().map_or(0, |state| state.elapsed_ms / 1000);
                        ui.monospace(format!("{:02}:{:02}", seconds / 60, seconds % 60));
                    });
                });
                self.toolbar(ui, state.as_ref());
                if !self.note.is_empty() {
                    ui.label(&self.note);
                }
                ui.separator();
                if let Some(state) = state.as_ref() {
                    if state.status == Status::Paused {
                        self.nav.clear();
                        ui.heading("Partie en pause");
                        ui.label("La grille est masquée. Utilisez Reprendre pour continuer.");
                    } else {
                        match state.status {
                            Status::Failed => { ui.heading("Partie perdue"); ui.label("Ce coup contredit la solution. Recommencez cette grille ou essayez-en une nouvelle."); }
                            Status::Won => { ui.heading("Grille résolue !"); }
                            Status::Playing | Status::Paused => {}
                        }
                        self.play(ui, state);
                    }
                } else {
                    self.nav.clear();
                    ui.heading("Aucune partie");
                    ui.label("Générez une nouvelle grille ou chargez un emplacement.");
                }
            });
        self.show_help(ui.ctx());
    }
}
