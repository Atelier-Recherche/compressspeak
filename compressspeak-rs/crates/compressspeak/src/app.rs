use crate::player::AudioPlayer;
use crate::settings::SettingsUi;
use crate::theme::apply_theme;
use crate::widgets::{
    action_button, bottom_progress_bar, delete_button, drop_zone, progress_ring, section_card,
    toggle_switch, DropZoneAction,
};
use cs_core::{
    data_dir, init_i18n, is_audio, set_language, spawn_convert, spawn_transcribe_only, tr,
    AppConfig, CleanMode, ItemStatus, Language, PipelineEvent, PipelineHandle, QueueItem,
    ThemePref, APP_NAME,
};
use cs_ffmpeg::{build_preview_command, ffmpeg_available, resolve_ffmpeg, run_command};
use cs_uvr5::is_available as uvr5_available; // always true (native ONNX)
use cs_whisper::{detection_status as whisper_detection_status, is_available as whisper_available};
use eframe::App;
use egui::{Align, Align2, Color32, Id, Layout, Order, RichText};
use std::path::PathBuf;
use std::time::Duration;

pub struct CompressSpeakApp {
    config: AppConfig,
    queue: Vec<QueueItem>,
    logs: Vec<String>,
    selected: Option<usize>,
    player: AudioPlayer,
    settings: SettingsUi,
    pipeline: Option<PipelineHandle>,
    busy: bool,
    /// Seek slider value while dragging (secs).
    seek_drag: Option<f32>,
    /// Pending Opus re-encode confirmation: `Some(process_all)`.
    opus_confirm: Option<bool>,
}

impl CompressSpeakApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        init_i18n();
        let config = AppConfig::load();
        set_language(config.language);
        apply_theme(&cc.egui_ctx, config.theme);
        let mut app = Self {
            config,
            queue: Vec::new(),
            logs: Vec::new(),
            selected: None,
            player: AudioPlayer::new(),
            settings: SettingsUi::default(),
            pipeline: None,
            busy: false,
            seek_drag: None,
            opus_confirm: None,
        };
        app.log(format!("{APP_NAME} — Rust / egui"));
        let data = data_dir();
        if ffmpeg_available(Some(&data)) {
            app.log(tr("status.ffmpeg_ok"));
        } else {
            app.log(tr("status.ffmpeg_missing"));
        }
        if cs_uvr5::model_is_ready(app.config.uvr5.model_file()) {
            app.log(tr("status.uvr5_ok"));
        } else {
            app.log(tr("status.uvr5_missing"));
        }
        if whisper_available(&app.config.whisper.cli_path) {
            app.log(format!(
                "{} — {}",
                tr("status.whisper_ok"),
                whisper_detection_status(&app.config.whisper.cli_path)
            ));
        } else {
            app.log(format!(
                "{} — {}",
                tr("status.whisper_missing"),
                whisper_detection_status(&app.config.whisper.cli_path)
            ));
        }
        app
    }

    fn log(&mut self, msg: impl AsRef<str>) {
        let msg = msg.as_ref().to_string();
        tracing::info!("{msg}");
        self.logs.push(msg);
        if self.logs.len() > 500 {
            self.logs.drain(0..self.logs.len() - 400);
        }
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut n = 0;
        for p in paths {
            if is_audio(&p) && !self.queue.iter().any(|q| q.path == p) {
                self.queue
                    .push(QueueItem::new(p, self.config.default_gain_db));
                n += 1;
            }
        }
        if n > 0 {
            self.log(format!("+{n} fichier(s)"));
        }
    }

    fn browse_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("Audio", cs_core::config::AUDIO_EXTENSIONS)
            .pick_files()
        {
            self.add_paths(files);
        }
    }

    fn poll_pipeline(&mut self, ctx: &egui::Context) {
        let events: Vec<PipelineEvent> = match self.pipeline.as_ref() {
            Some(handle) => {
                let mut v = Vec::new();
                while let Ok(ev) = handle.rx.try_recv() {
                    v.push(ev);
                }
                v
            }
            None => return,
        };
        let mut finished = false;
        for ev in events {
            match ev {
                PipelineEvent::Log(s) => self.log(s),
                PipelineEvent::ItemProgress { index, progress } => {
                    if let Some(item) = self.queue.get_mut(index) {
                        item.progress = progress;
                    }
                }
                PipelineEvent::ItemStatus {
                    index,
                    status,
                    error,
                } => {
                    if let Some(item) = self.queue.get_mut(index) {
                        item.status = status;
                        item.error = error;
                    }
                }
                PipelineEvent::Finished { cancelled } => {
                    self.busy = false;
                    finished = true;
                    self.log(if cancelled { "Annulé." } else { "Terminé." });
                }
            }
        }
        if finished {
            self.pipeline = None;
        } else if self.busy {
            ctx.request_repaint();
        }
    }

    fn overall_progress(&self) -> (f32, String) {
        if !self.busy || self.queue.is_empty() {
            return (0.0, tr("progress.idle"));
        }
        let n = self.queue.len() as f32;
        let sum: f32 = self
            .queue
            .iter()
            .map(|q| match q.status {
                ItemStatus::Done => 100.0,
                ItemStatus::Failed | ItemStatus::Cancelled => 100.0,
                ItemStatus::Running => q.progress as f32,
                ItemStatus::Pending => 0.0,
            })
            .sum();
        let frac = (sum / (n * 100.0)).clamp(0.0, 1.0);
        let running = self
            .queue
            .iter()
            .find(|q| q.status == ItemStatus::Running)
            .map(|q| q.display_name())
            .unwrap_or_else(|| tr("progress.bar"));
        let pct = (frac * 100.0).round() as u32;
        (frac, format!("{running} — {pct}%"))
    }

    fn needs_opus_confirm(&self) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        let any_dsp = self
            .queue
            .iter()
            .any(|q| q.isolate || q.warmth || q.clean != CleanMode::Off);
        if any_dsp {
            return false;
        }
        let any_opus = self.queue.iter().any(|q| {
            q.path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("opus"))
        });
        let any_transcribe = self.queue.iter().any(|q| q.transcribe);
        any_opus || any_transcribe
    }

    fn request_convert(&mut self, process_all: bool) {
        if self.busy || self.queue.is_empty() {
            return;
        }
        if self.needs_opus_confirm() {
            self.opus_confirm = Some(process_all);
            return;
        }
        self.start_convert(process_all);
    }

    fn start_convert(&mut self, process_all: bool) {
        if self.busy || self.queue.is_empty() {
            return;
        }
        self.opus_confirm = None;
        let items: Vec<_> = self
            .queue
            .iter()
            .enumerate()
            .map(|(i, q)| (i, q.clone()))
            .collect();
        for item in &mut self.queue {
            item.status = ItemStatus::Pending;
            item.progress = 0;
            item.error = None;
        }
        self.busy = true;
        self.log(if process_all {
            "Tout traiter…"
        } else {
            "Conversion…"
        });
        let _ = self.config.save();
        self.pipeline = Some(spawn_convert(items, self.config.clone(), process_all));
    }

    fn start_transcribe(&mut self) {
        if self.busy {
            return;
        }
        self.opus_confirm = None;
        let items: Vec<_> = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, q)| q.transcribe)
            .map(|(i, q)| (i, q.clone()))
            .collect();
        if items.is_empty() {
            self.log(tr("transcribe.none_checked"));
            return;
        }
        if !whisper_available(&self.config.whisper.cli_path) {
            self.log(format!(
                "{} — {}",
                tr("status.whisper_missing"),
                whisper_detection_status(&self.config.whisper.cli_path)
            ));
            return;
        }
        self.busy = true;
        self.log("Transcription…");
        let _ = self.config.save();
        self.pipeline = Some(spawn_transcribe_only(items, self.config.clone()));
    }

    fn opus_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(process_all) = self.opus_confirm else {
            return;
        };
        let mut open = true;
        let mut choice: Option<&'static str> = None;
        egui::Window::new(tr("opus.confirm_title"))
            .id(Id::new("cs_opus_confirm"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("opus.confirm_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(tr("opus.confirm_reencode")).strong())
                        .clicked()
                    {
                        choice = Some("reencode");
                    }
                    let can_tr = self.queue.iter().any(|q| q.transcribe);
                    if ui
                        .add_enabled(can_tr, egui::Button::new(tr("opus.confirm_transcribe_only")))
                        .clicked()
                    {
                        choice = Some("transcribe");
                    }
                    if ui.button(tr("btn.cancel")).clicked() {
                        choice = Some("cancel");
                    }
                });
            });
        if !open && choice.is_none() {
            choice = Some("cancel");
        }
        match choice {
            Some("reencode") => self.start_convert(process_all),
            Some("transcribe") => self.start_transcribe(),
            Some("cancel") => self.opus_confirm = None,
            _ => {}
        }
    }

    fn cancel(&mut self) {
        if let Some(h) = &self.pipeline {
            h.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            self.log("Annulation demandée…");
        }
    }

    fn select_row(&mut self, i: usize) {
        self.selected = Some(i);
        let path = self.queue[i].path.clone();
        match self.player.load(&path) {
            Ok(()) => {}
            Err(e) => self.log(format!("Lecture: {e}")),
        }
    }

    fn preview_5s(&mut self) {
        let Some(idx) = self.selected else {
            self.log(tr("player.no_file"));
            return;
        };
        let Some(item) = self.queue.get(idx).cloned() else {
            return;
        };
        let start_sec = self.player.position_secs() as f64;
        let return_pos = self.player.position();
        let data = data_dir();
        let Ok(ffmpeg) = resolve_ffmpeg(Some(&data)) else {
            self.log(tr("status.ffmpeg_missing"));
            return;
        };
        let out = data.join("preview_5s.opus");
        let cmd = build_preview_command(
            &ffmpeg,
            &item.path,
            &out,
            start_sec,
            5.0,
            item.gain_db,
            &self.config.conversion,
        );
        match run_command(&cmd) {
            Ok(()) => match self.player.load_preview(&out, return_pos) {
                Ok(()) => {
                    self.player.play();
                    self.log(format!(
                        "Aperçu 5 s @ {}",
                        AudioPlayer::format_time(start_sec as f32)
                    ));
                }
                Err(e) => self.log(format!("Lecture: {e}")),
            },
            Err(e) => self.log(format!("Preview: {e}")),
        }
    }

    fn return_full(&mut self) {
        let Some(path) = self.player.full_path.clone() else {
            return;
        };
        let pos = self.player.return_position;
        match self.player.load(&path) {
            Ok(()) => {
                self.player.seek(pos);
                self.player.play();
                self.log(tr("player.return_full"));
            }
            Err(e) => self.log(format!("Lecture: {e}")),
        }
    }

    fn bulk_set(&mut self, field: BulkField, value: bool) {
        let uvr_ok = uvr5_available(&self.config.uvr5);
        for item in &mut self.queue {
            match field {
                BulkField::Isolate if uvr_ok => {
                    item.isolate = value;
                    if value {
                        item.clean = CleanMode::Off;
                    }
                }
                BulkField::Transcribe => item.transcribe = value,
                BulkField::Warmth => item.warmth = value,
                _ => {}
            }
        }
    }

    fn bulk_set_clean(&mut self, mode: CleanMode) {
        for item in &mut self.queue {
            if !item.isolate {
                item.clean = mode;
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new(APP_NAME).strong().size(20.0));
            ui.label(
                RichText::new(if self.busy { "● en cours" } else { "○ prêt" })
                    .size(13.0)
                    .color(if self.busy {
                        Color32::from_rgb(0, 180, 100)
                    } else {
                        Color32::GRAY
                    }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.settings_menu(ui);
            });
        });
    }

    /// Menu paramètres ancré à droite, popup ouverte vers la gauche.
    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        let btn = ui.add(
            egui::Button::new(format!("⚙ {}", tr("settings.menu")))
                .min_size(egui::vec2(0.0, 28.0)),
        );
        let popup_id = Id::new("cs_settings_menu_popup");
        let just_toggled = btn.clicked();
        if just_toggled {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }

        let open = ui.memory(|m| m.is_popup_open(popup_id));
        if !open {
            return;
        }

        let mut open_conversion = false;
        let mut open_warmth = false;
        let mut open_dpdf = false;
        let mut open_inworld = false;
        let mut open_whisper = false;
        let mut open_uvr5 = false;
        let mut open_deps = false;
        let mut theme_change: Option<ThemePref> = None;
        let mut lang_change: Option<Language> = None;
        let mut show_logs = self.config.show_logs;
        let mut logs_changed = false;
        let mut close_popup = false;
        let current_theme = self.config.theme;
        let current_lang = self.config.language;

        // Pivot RIGHT_TOP : le menu s'étend vers la gauche sous le bouton
        let popup_resp = egui::Area::new(Id::new("cs_settings_menu_area"))
            .order(Order::Foreground)
            .fixed_pos(btn.rect.right_bottom() + egui::vec2(0.0, 2.0))
            .pivot(Align2::RIGHT_TOP)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(210.0);
                    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                        if ui.button(tr("settings.conversion")).clicked() {
                            open_conversion = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.warmth")).clicked() {
                            open_warmth = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.dpdf")).clicked() {
                            open_dpdf = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.inworld")).clicked() {
                            open_inworld = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.whisper")).clicked() {
                            open_whisper = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.uvr5")).clicked() {
                            open_uvr5 = true;
                            close_popup = true;
                        }
                        if ui.button(tr("settings.deps")).clicked() {
                            open_deps = true;
                            close_popup = true;
                        }
                        ui.separator();
                        if ui
                            .checkbox(&mut show_logs, tr("settings.show_logs"))
                            .changed()
                        {
                            logs_changed = true;
                        }
                        ui.separator();
                        ui.label(RichText::new("Thème").strong().size(13.0));
                        for (pref, label) in [
                            (ThemePref::Dark, tr("theme.dark")),
                            (ThemePref::Light, tr("theme.light")),
                            (ThemePref::System, tr("theme.system")),
                        ] {
                            if ui.selectable_label(current_theme == pref, label).clicked() {
                                theme_change = Some(pref);
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new("Langue").strong().size(13.0));
                        for (lang, key) in [
                            (Language::Fr, "lang.fr"),
                            (Language::En, "lang.en"),
                            (Language::De, "lang.de"),
                            (Language::Es, "lang.es"),
                        ] {
                            if ui
                                .selectable_label(current_lang == lang, tr(key))
                                .clicked()
                            {
                                lang_change = Some(lang);
                            }
                        }
                    });
                });
            });

        if !just_toggled {
            let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
            if ui.ctx().input(|i| i.pointer.any_click()) {
                if let Some(p) = pointer {
                    if !btn.rect.contains(p) && !popup_resp.response.rect.contains(p) {
                        close_popup = true;
                    }
                }
            }
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            close_popup = true;
        }
        if close_popup {
            ui.memory_mut(|m| m.close_popup());
        }

        if open_conversion {
            self.settings.show_conversion = true;
        }
        if open_warmth {
            self.settings.show_warmth = true;
        }
        if open_dpdf {
            self.settings.show_dpdf = true;
        }
        if open_inworld {
            self.settings.show_inworld = true;
        }
        if open_whisper {
            self.settings.show_whisper = true;
        }
        if open_uvr5 {
            self.settings.show_uvr5 = true;
        }
        if open_deps {
            self.settings.show_deps = true;
        }
        if logs_changed {
            self.config.show_logs = show_logs;
            let _ = self.config.save();
        }
        if let Some(pref) = theme_change {
            self.config.theme = pref;
            apply_theme(ui.ctx(), pref);
            let _ = self.config.save();
        }
        if let Some(lang) = lang_change {
            self.config.language = lang;
            set_language(lang);
            let _ = self.config.save();
        }
    }

    fn queue_panel(&mut self, ui: &mut egui::Ui) {
        let uvr_ok = uvr5_available(&self.config.uvr5);

        section_card(ui, &tr("queue.section"), |ui| {
            match drop_zone(ui, &tr("queue.empty_hint"), &tr("btn.add_files")) {
                DropZoneAction::Browse => self.browse_files(),
                DropZoneAction::None => {}
            }
            ui.add_space(6.0);

            if self.queue.is_empty() {
                return;
            }

            // Colonnes fixes partagées : légendes / Tout / lignes alignées
            // Ordre de tâche UI : Isoler → Transcrire → Mixage → Nettoyer
            let cols = QueueCols::from_available(ui.available_width());

            self.queue_header_and_bulk(ui, &cols, uvr_ok);

            egui::ScrollArea::vertical()
                .id_salt("queue_rows_scroll")
                .max_height(260.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let mut remove = None;
                    let mut load_idx = None;
                    let len = self.queue.len();
                    for i in 0..len {
                        let selected = self.selected == Some(i);
                        let spinning = self.queue[i].status == ItemStatus::Running;
                        let progress = self.queue[i].progress;

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = cols.gap;
                            queue_cell(ui, cols.prog, |ui| {
                                progress_ring(ui, progress, spinning);
                            });
                            queue_cell(ui, cols.file, |ui| {
                                let name = self.queue[i].display_name();
                                let resp = ui.add_sized(
                                    egui::vec2(cols.file, 28.0),
                                    egui::SelectableLabel::new(
                                        selected,
                                        RichText::new(name).size(14.5).color(if selected {
                                            Color32::from_rgb(100, 180, 255)
                                        } else {
                                            ui.visuals().text_color()
                                        }),
                                    ),
                                );
                                if resp.clicked() {
                                    load_idx = Some(i);
                                }
                            });
                            queue_cell(ui, cols.gain, |ui| {
                                ui.add_sized(
                                    egui::vec2(cols.gain, 28.0),
                                    egui::DragValue::new(&mut self.queue[i].gain_db)
                                        .range(-30..=60)
                                        .suffix(" dB"),
                                );
                            });
                            // Isoler → Transcrire → Mixage → Nettoyer
                            queue_cell(ui, cols.toggle, |ui| {
                                ui.vertical_centered(|ui| {
                                    let item = &mut self.queue[i];
                                    let r = toggle_switch(ui, &mut item.isolate, uvr_ok);
                                    if r.changed() && item.isolate {
                                        item.clean = CleanMode::Off;
                                    }
                                });
                            });
                            queue_cell(ui, cols.toggle, |ui| {
                                ui.vertical_centered(|ui| {
                                    toggle_switch(ui, &mut self.queue[i].transcribe, true);
                                });
                            });
                            queue_cell(ui, cols.toggle, |ui| {
                                ui.vertical_centered(|ui| {
                                    toggle_switch(ui, &mut self.queue[i].warmth, true);
                                });
                            });
                            queue_cell(ui, cols.toggle, |ui| {
                                ui.vertical_centered(|ui| {
                                    let isolate = self.queue[i].isolate;
                                    clean_mode_combo(ui, &mut self.queue[i].clean, !isolate);
                                });
                            });
                            queue_cell(ui, cols.del, |ui| {
                                if delete_button(ui).clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                    }

                    if let Some(i) = load_idx {
                        self.select_row(i);
                    }
                    if let Some(i) = remove {
                        self.queue.remove(i);
                        if self.selected == Some(i) {
                            self.selected = None;
                            self.player.stop();
                        } else if let Some(s) = self.selected {
                            if s > i {
                                self.selected = Some(s - 1);
                            }
                        }
                    }
                });
        });
    }

    fn queue_header_and_bulk(&mut self, ui: &mut egui::Ui, cols: &QueueCols, uvr_ok: bool) {
        // Ligne légendes
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = cols.gap;
            queue_cell(ui, cols.prog, |_| {});
            queue_cell(ui, cols.file, |ui| {
                ui.label(RichText::new(tr("queue.header_file")).strong().size(13.5));
            });
            queue_cell(ui, cols.gain, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(tr("queue.header_gain")).strong().size(13.5));
                });
            });
            for key in [
                "queue.header_isolate",
                "queue.header_transcribe",
                "queue.header_warmth",
                "queue.header_dpdf",
            ] {
                queue_cell(ui, cols.toggle, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(tr(key)).strong().size(13.0));
                    });
                });
            }
            queue_cell(ui, cols.del, |_| {});
        });

        // Ligne « Tout » — mêmes colonnes toggles
        let all_iso = self.queue.iter().all(|q| q.isolate);
        let all_tr = self.queue.iter().all(|q| q.transcribe);
        let all_w = self.queue.iter().all(|q| q.warmth);
        let bulk_clean = self
            .queue
            .iter()
            .find(|q| !q.isolate)
            .map(|q| q.clean)
            .unwrap_or(CleanMode::Off);
        let mut iso = all_iso;
        let mut tr_ = all_tr;
        let mut w = all_w;
        let mut clean = bulk_clean;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = cols.gap;
            queue_cell(ui, cols.prog, |_| {});
            queue_cell(ui, cols.file, |ui| {
                ui.label(RichText::new(tr("queue.bulk")).strong().size(13.5));
            });
            queue_cell(ui, cols.gain, |_| {});
            queue_cell(ui, cols.toggle, |ui| {
                ui.vertical_centered(|ui| {
                    if toggle_switch(ui, &mut iso, uvr_ok).changed() {
                        self.bulk_set(BulkField::Isolate, iso);
                    }
                });
            });
            queue_cell(ui, cols.toggle, |ui| {
                ui.vertical_centered(|ui| {
                    if toggle_switch(ui, &mut tr_, true).changed() {
                        self.bulk_set(BulkField::Transcribe, tr_);
                    }
                });
            });
            queue_cell(ui, cols.toggle, |ui| {
                ui.vertical_centered(|ui| {
                    if toggle_switch(ui, &mut w, true).changed() {
                        self.bulk_set(BulkField::Warmth, w);
                    }
                });
            });
            queue_cell(ui, cols.toggle, |ui| {
                ui.vertical_centered(|ui| {
                    if clean_mode_combo(ui, &mut clean, true).changed() {
                        self.bulk_set_clean(clean);
                    }
                });
            });
            queue_cell(ui, cols.del, |_| {});
        });
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);
    }

    fn player_panel(&mut self, ui: &mut egui::Ui) {
        section_card(ui, &tr("player.section"), |ui| {
            let name = if self.selected.is_some() {
                self.player
                    .current
                    .as_ref()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| tr("player.no_file"))
            } else {
                tr("player.no_file")
            };
            ui.label(RichText::new(name).strong().size(15.0));
            if self.player.is_preview {
                ui.label(
                    RichText::new("aperçu 5 s (effets Opus)")
                        .size(13.0)
                        .color(Color32::from_rgb(0, 160, 220)),
                );
            }

            ui.add_space(2.0);

            let dur = self.player.duration_secs();
            let mut pos = self.seek_drag.unwrap_or(self.player.position_secs());
            pos = pos.clamp(0.0, dur);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(AudioPlayer::format_time(pos))
                        .monospace()
                        .size(13.0),
                );
                let slider = ui.add_enabled(
                    self.player.has_track() && dur > 0.05,
                    egui::Slider::new(&mut pos, 0.0..=dur)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if slider.dragged() || slider.changed() {
                    self.seek_drag = Some(pos);
                }
                if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                    self.player.seek(Duration::from_secs_f32(pos));
                    self.seek_drag = None;
                }
                ui.label(
                    RichText::new(AudioPlayer::format_time(dur))
                        .monospace()
                        .size(13.0),
                );
            });

            if !self.player.is_paused() {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let play_label = if self.player.is_paused() {
                    tr("player.play")
                } else {
                    tr("player.pause")
                };
                if ui
                    .add_enabled(self.player.has_track(), {
                        egui::Button::new(play_label).min_size(egui::vec2(88.0, 28.0))
                    })
                    .clicked()
                {
                    self.player.toggle();
                }
                if ui
                    .add_enabled(self.player.has_track(), {
                        egui::Button::new(tr("player.stop")).min_size(egui::vec2(0.0, 28.0))
                    })
                    .clicked()
                {
                    self.player.stop();
                    self.seek_drag = None;
                }
                if ui
                    .add_enabled(self.selected.is_some(), {
                        egui::Button::new(tr("player.preview")).min_size(egui::vec2(0.0, 28.0))
                    })
                    .clicked()
                {
                    self.preview_5s();
                }
                if ui
                    .add_enabled(self.player.is_preview && self.player.full_path.is_some(), {
                        egui::Button::new(tr("player.return_full")).min_size(egui::vec2(0.0, 28.0))
                    })
                    .clicked()
                {
                    self.return_full();
                }
            });
        });
    }

    fn output_and_actions(&mut self, ui: &mut egui::Ui) {
        section_card(ui, &tr("form.section"), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("form.suffix_label"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.clean_suffix)
                        .desired_width(110.0)
                        .hint_text("_clean"),
                );
                ui.add_space(10.0);
                let mut keep = self.config.keep_flac;
                if toggle_switch(ui, &mut keep, true).changed() {
                    self.config.keep_flac = keep;
                }
                ui.label(tr("form.keep_flac"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("form.output_dir"));
                let btn_w = 88.0;
                let edit_w = (ui.available_width() - btn_w - 8.0).max(80.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.output_dir)
                        .desired_width(edit_w)
                        .hint_text(tr("form.output_placeholder")),
                );
                if action_button(ui, tr("btn.choose")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.config.output_dir = p.display().to_string();
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let primary = egui::Button::new(RichText::new(tr("btn.convert")).strong())
                    .fill(Color32::from_rgb(0, 120, 215))
                    .min_size(egui::vec2(140.0, 28.0));
                if ui
                    .add_enabled(!self.busy && !self.queue.is_empty(), primary)
                    .clicked()
                {
                    self.request_convert(false);
                }
                if ui
                    .add_enabled(!self.busy && !self.queue.is_empty(), {
                        egui::Button::new(tr("btn.process_all")).min_size(egui::vec2(110.0, 28.0))
                    })
                    .clicked()
                {
                    self.request_convert(true);
                }
                if ui
                    .add_enabled(!self.busy, {
                        egui::Button::new(tr("btn.transcribe")).min_size(egui::vec2(100.0, 28.0))
                    })
                    .clicked()
                {
                    self.start_transcribe();
                }
                if ui
                    .add_enabled(self.busy, {
                        egui::Button::new(tr("btn.cancel")).min_size(egui::vec2(90.0, 28.0))
                    })
                    .clicked()
                {
                    self.cancel();
                }
                if ui
                    .add_enabled(!self.busy && !self.queue.is_empty(), {
                        egui::Button::new(tr("btn.clear_queue")).min_size(egui::vec2(0.0, 28.0))
                    })
                    .clicked()
                {
                    self.queue.clear();
                    self.selected = None;
                    self.player.stop();
                }
            });
        });
    }

    fn logs_panel(&mut self, ui: &mut egui::Ui) {
        section_card(ui, &tr("logs.title"), |ui| {
            ui.horizontal(|ui| {
                if ui.small_button(tr("logs.copy")).clicked() {
                    ui.ctx().copy_text(self.logs.join("\n"));
                    self.log(tr("logs.copied"));
                }
                if ui.small_button(tr("logs.clear")).clicked() {
                    self.logs.clear();
                }
            });
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(220.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let text = self.logs.join("\n");
                    ui.add(
                        egui::Label::new(RichText::new(text).monospace().size(13.0))
                            .selectable(true)
                            .wrap(),
                    );
                });
        });
    }
}

enum BulkField {
    Isolate,
    Transcribe,
    Warmth,
}

fn clean_mode_combo(ui: &mut egui::Ui, mode: &mut CleanMode, enabled: bool) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(ui.id().with("clean_mode"))
            .width(ui.available_width().max(64.0))
            .selected_text(mode.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(mode, CleanMode::Off, CleanMode::Off.label());
                ui.selectable_value(mode, CleanMode::Dpdf, CleanMode::Dpdf.label());
                ui.selectable_value(mode, CleanMode::Inworld, CleanMode::Inworld.label());
            })
            .response
    })
    .inner
}

/// Largeurs de colonnes partagées entre en-tête, « Tout » et lignes fichier.
struct QueueCols {
    prog: f32,
    file: f32,
    gain: f32,
    toggle: f32,
    del: f32,
    gap: f32,
}

impl QueueCols {
    fn from_available(avail: f32) -> Self {
        let gap = 6.0;
        let prog = 28.0;
        let gain = 70.0;
        let del = 28.0;
        let n_gaps = 7.0;
        let fixed_base = prog + gain + del + gap * n_gaps;
        // 4 colonnes toggles : Isoler / Transcrire / Mixage / Nettoyer (combo)
        let mut toggle = 88.0;
        let mut file = avail - fixed_base - toggle * 4.0;
        if file < 100.0 {
            let room = (avail - fixed_base - 100.0).max(0.0);
            toggle = (room / 4.0).clamp(56.0, 88.0);
            file = (avail - fixed_base - toggle * 4.0).max(80.0);
        }
        // Ne jamais dépasser la largeur dispo (évite crop du cadre parent)
        let total = prog + file + gain + toggle * 4.0 + del + gap * n_gaps;
        if total > avail && avail > 0.0 {
            file = (file - (total - avail)).max(60.0);
        }
        Self {
            prog,
            file,
            gain,
            toggle,
            del,
            gap,
        }
    }
}

fn queue_cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y.max(28.0)),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            add(ui);
        },
    );
}

impl App for CompressSpeakApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_pipeline(ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped);
        }

        self.settings.show_all(ctx, &mut self.config);
        self.opus_confirm_dialog(ctx);

        let (frac, progress_label) = self.overall_progress();
        let busy = self.busy;

        egui::TopBottomPanel::bottom("cs_progress_bar")
            .exact_height(32.0)
            .show_separator_line(true)
            .show(ctx, |ui| {
                bottom_progress_bar(ui, frac, &progress_label, busy);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Borne la largeur pour que les cadres ne débordent pas / ne soient pas croppés
            let content_w = ui.available_width();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_width(content_w)
                .show(ui, |ui| {
                    ui.set_max_width(content_w);
                    self.top_bar(ui);
                    ui.add_space(6.0);
                    self.queue_panel(ui);
                    ui.add_space(6.0);
                    self.player_panel(ui);
                    ui.add_space(6.0);
                    self.output_and_actions(ui);
                    if self.config.show_logs {
                        ui.add_space(6.0);
                        self.logs_panel(ui);
                    }
                    ui.add_space(4.0);
                });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.config.save();
    }
}
