use cs_core::{data_dir, tr, AppConfig};
use cs_dpdf::{model_labels_for_sr, SAMPLE_RATES};
use cs_ffmpeg::{ffmpeg_available, ConversionParams};
use cs_uvr5::{is_available as uvr5_available, AVAILABLE_STEMS};
use cs_warmth::WarmthParams;
use cs_whisper::{
    detection_status, is_available as whisper_available, list_discovered_models,
    openwhispr_install_dir, openwhispr_models_dir, vibe_install_dir, TranscriptFormat, MODEL_NAMES,
};
use crossbeam_channel::{unbounded, Receiver};
use egui::{Color32, Context, Id, RichText};
use std::path::PathBuf;

#[derive(Debug, Clone)]
enum DepsJob {
    Ffmpeg,
    WhisperCli,
    WhisperModel(String),
    DpdfModel(String),
    Uvr5Pip,
}

enum DepsEvent {
    Done { ok: bool, msg: String },
}

pub struct SettingsUi {
    pub show_conversion: bool,
    pub show_warmth: bool,
    pub show_dpdf: bool,
    pub show_whisper: bool,
    pub show_uvr5: bool,
    pub show_deps: bool,
    deps_busy: bool,
    deps_status: String,
    deps_rx: Option<Receiver<DepsEvent>>,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            show_conversion: false,
            show_warmth: false,
            show_dpdf: false,
            show_whisper: false,
            show_uvr5: false,
            show_deps: false,
            deps_busy: false,
            deps_status: String::new(),
            deps_rx: None,
        }
    }
}

impl SettingsUi {
    pub fn show_all(&mut self, ctx: &Context, config: &mut AppConfig) {
        self.poll_deps(ctx);
        self.conversion(ctx, config);
        self.warmth(ctx, config);
        self.dpdf(ctx, config);
        self.whisper(ctx, config);
        self.uvr5(ctx, config);
        self.dependencies(ctx, config);
    }

    fn poll_deps(&mut self, ctx: &Context) {
        let Some(rx) = &self.deps_rx else {
            return;
        };
        let mut done = None;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                DepsEvent::Done { ok, msg } => done = Some((ok, msg)),
            }
        }
        if let Some((ok, msg)) = done {
            self.deps_busy = false;
            self.deps_rx = None;
            self.deps_status = if ok {
                format!("✓ {msg}")
            } else {
                format!("✗ {msg}")
            };
        } else if self.deps_busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn start_deps_job(&mut self, job: DepsJob) {
        if self.deps_busy {
            return;
        }
        self.deps_busy = true;
        self.deps_status = match &job {
            DepsJob::Ffmpeg => tr("deps.installing_ffmpeg").to_string(),
            DepsJob::WhisperCli => tr("deps.installing_whisper_cli").to_string(),
            DepsJob::WhisperModel(m) => format!("{} ({m})…", tr("deps.installing_model")),
            DepsJob::DpdfModel(m) => format!("{} ({m})…", tr("deps.installing_dpdf")),
            DepsJob::Uvr5Pip => tr("deps.installing_uvr5").to_string(),
        };
        let (tx, rx) = unbounded();
        self.deps_rx = Some(rx);
        std::thread::spawn(move || {
            let result = match job {
                DepsJob::Ffmpeg => cs_ffmpeg::install_ffmpeg(&data_dir())
                    .map(|p| format!("FFmpeg → {}", p.display()))
                    .map_err(|e| e.to_string()),
                DepsJob::WhisperCli => cs_whisper::install_whisper_cli()
                    .map(|p| format!("whisper-cli → {}", p.display()))
                    .map_err(|e| e.to_string()),
                DepsJob::WhisperModel(m) => cs_whisper::ensure_model(&m)
                    .map(|p| format!("modèle → {}", p.display()))
                    .map_err(|e| e.to_string()),
                DepsJob::DpdfModel(m) => cs_dpdf::ensure_model(&m)
                    .map(|p| format!("DPDF → {}", p.display()))
                    .map_err(|e| e.to_string()),
                DepsJob::Uvr5Pip => cs_uvr5::install_via_pip().map_err(|e| e.to_string()),
            };
            let _ = match result {
                Ok(msg) => tx.send(DepsEvent::Done { ok: true, msg }),
                Err(msg) => tx.send(DepsEvent::Done { ok: false, msg }),
            };
        });
    }

    fn open_url(url: &str) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
    }

    fn dependencies(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_deps;
        let data = data_dir();
        let ff_ok = ffmpeg_available(Some(&data));
        let ff_path = cs_ffmpeg::resolve_ffmpeg(Some(&data))
            .ok()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let wh_ok = whisper_available(&config.whisper.cli_path);
        let wh_detail = detection_status(&config.whisper.cli_path);
        let ow = openwhispr_install_dir().is_some() || openwhispr_models_dir().is_some();
        let vibe = vibe_install_dir().is_some();
        let models = list_discovered_models();
        let uvr_ok = uvr5_available(&config.uvr5);
        let dpdf_model = config.dpdf.model_id();
        let dpdf_local = cs_dpdf::find_local_model(&dpdf_model);
        let dpdf_ok = dpdf_local.is_some();
        let dpdf_detail = dpdf_local
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                format!(
                    "absent — sera téléchargé vers {}",
                    cs_dpdf::models_dir()
                        .join(format!("{dpdf_model}.onnx"))
                        .display()
                )
            });
        let busy = self.deps_busy;
        let status = self.deps_status.clone();
        let whisper_model = config.whisper.model.clone();

        let mut do_ffmpeg = false;
        let mut do_whisper_cli = false;
        let mut do_whisper_model = false;
        let mut do_dpdf = false;
        let mut do_uvr5 = false;
        let mut open_vibe = false;
        let mut open_ow = false;

        egui::Window::new(tr("settings.deps"))
            .id(Id::new("cs_settings_deps"))
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(tr("deps.intro"))
                        .weak()
                        .small(),
                );
                ui.add_space(6.0);

                let row = |ui: &mut egui::Ui,
                           title: &str,
                           ok: bool,
                           detail: &str,
                           btn: String,
                           enabled: bool|
                 -> bool {
                    let mut clicked = false;
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let (mark, color) = if ok {
                                ("●", Color32::from_rgb(60, 170, 90))
                            } else {
                                ("○", Color32::from_rgb(200, 120, 60))
                            };
                            ui.label(RichText::new(mark).color(color).strong());
                            ui.vertical(|ui| {
                                ui.label(RichText::new(title).strong());
                                if !detail.is_empty() {
                                    ui.label(RichText::new(detail).small().weak());
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    clicked = ui
                                        .add_enabled(
                                            enabled && !busy,
                                            egui::Button::new(btn),
                                        )
                                        .clicked();
                                },
                            );
                        });
                    });
                    clicked
                };

                if row(
                    ui,
                    "FFmpeg",
                    ff_ok,
                    if ff_ok {
                        ff_path.as_str()
                    } else {
                        "requis pour conversion / aperçu"
                    },
                    if ff_ok {
                        tr("deps.reinstall")
                    } else {
                        tr("deps.install")
                    },
                    true,
                ) {
                    do_ffmpeg = true;
                }

                if row(
                    ui,
                    "Whisper CLI",
                    wh_ok,
                    &wh_detail,
                    if wh_ok {
                        tr("deps.reinstall")
                    } else {
                        tr("deps.install")
                    },
                    cfg!(windows),
                ) {
                    do_whisper_cli = true;
                }

                let model_detail = {
                    let local: Vec<_> = models
                        .iter()
                        .map(|m| format!("{} [{}]", m.name, m.source_label()))
                        .collect();
                    if local.is_empty() {
                        format!("{} — aucun modèle local", whisper_model)
                    } else {
                        format!(
                            "sélection: {whisper_model} · trouvés: {}",
                            local.join(", ")
                        )
                    }
                };
                let model_ok = models.iter().any(|m| {
                    m.name == whisper_model
                        || m.path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.contains(&whisper_model))
                }) || PathBuf::from(&whisper_model).is_file();
                let model_title = format!("{} ({whisper_model})", tr("deps.whisper_model"));
                if row(
                    ui,
                    &model_title,
                    model_ok,
                    &model_detail,
                    tr("deps.download"),
                    true,
                ) {
                    do_whisper_model = true;
                }

                let dpdf_title = format!("DPDF ({dpdf_model})");
                if row(
                    ui,
                    &dpdf_title,
                    dpdf_ok,
                    &dpdf_detail,
                    tr("deps.download"),
                    true,
                ) {
                    do_dpdf = true;
                }

                if row(
                    ui,
                    "UVR5 / audio-separator",
                    uvr_ok,
                    if uvr_ok {
                        "détecté"
                    } else {
                        "optionnel — isolation de voix"
                    },
                    tr("deps.install_pip"),
                    true,
                ) {
                    do_uvr5 = true;
                }

                ui.separator();
                ui.label(RichText::new(tr("deps.external")).strong());
                ui.horizontal(|ui| {
                    ui.label(if ow {
                        RichText::new("OpenWhispr ●").color(Color32::from_rgb(60, 170, 90))
                    } else {
                        RichText::new("OpenWhispr ○").color(Color32::GRAY)
                    });
                    if ui.button(tr("deps.open_site")).clicked() {
                        open_ow = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(if vibe {
                        RichText::new("Vibe ●").color(Color32::from_rgb(60, 170, 90))
                    } else {
                        RichText::new("Vibe ○").color(Color32::GRAY)
                    });
                    if ui.button(tr("deps.open_site")).clicked() {
                        open_vibe = true;
                    }
                });

                if !status.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(&status).small());
                }
                if busy {
                    ui.spinner();
                    ui.label(tr("deps.busy"));
                }
            });

        self.show_deps = open;
        if do_ffmpeg {
            let dest_exe = data
                .join("ffmpeg")
                .join(if cfg!(windows) {
                    "windows"
                } else if cfg!(target_os = "macos") {
                    "macos"
                } else {
                    "linux"
                })
                .join(if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                });
            let _ = std::fs::remove_file(&dest_exe);
            self.start_deps_job(DepsJob::Ffmpeg);
        }
        if do_whisper_cli {
            // Allow reinstall by deleting vendor cli
            let cli = cs_whisper::data_root().join("whisper").join(if cfg!(windows) {
                "whisper-cli.exe"
            } else {
                "whisper-cli"
            });
            let _ = std::fs::remove_file(&cli);
            self.start_deps_job(DepsJob::WhisperCli);
        }
        if do_whisper_model {
            self.start_deps_job(DepsJob::WhisperModel(whisper_model));
        }
        if do_dpdf {
            self.start_deps_job(DepsJob::DpdfModel(dpdf_model));
        }
        if do_uvr5 {
            self.start_deps_job(DepsJob::Uvr5Pip);
        }
        if open_ow {
            Self::open_url("https://www.openwhispr.com/");
        }
        if open_vibe {
            Self::open_url("https://github.com/thewh1teagle/vibe/releases");
        }
    }

    fn conversion(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_conversion;
        egui::Window::new("Conversion Opus")
            .id(Id::new("cs_settings_conversion_opus"))
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Presets FFmpeg → libopus (voip)")
                        .strong()
                        .size(14.0),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("preset.audiobook")).clicked() {
                        config.conversion = ConversionParams::preset_audiobook();
                    }
                    if ui.button(tr("preset.podcast")).clicked() {
                        config.conversion = ConversionParams::preset_podcast();
                    }
                    if ui.button(tr("preset.voice_chat")).clicked() {
                        config.conversion = ConversionParams::preset_voice_chat();
                    }
                    if ui.button(tr("preset.voice_note")).clicked() {
                        config.conversion = ConversionParams::preset_voice_note();
                    }
                });
                ui.separator();
                ui.label(RichText::new("Réduction de bruit (afftdn)").strong());
                ui.add(
                    egui::Slider::new(&mut config.conversion.afftdn_nf, -50..=0)
                        .text("nf (dB)"),
                );
                ui.checkbox(
                    &mut config.conversion.skip_afftdn,
                    "Désactiver afftdn",
                );
                ui.separator();
                ui.label(RichText::new("Compresseur dynamique").strong());
                ui.add(
                    egui::Slider::new(&mut config.conversion.comp_threshold_db, -40.0..=0.0)
                        .text("seuil (dB)"),
                );
                ui.add(
                    egui::Slider::new(&mut config.conversion.comp_ratio, 1.0..=10.0)
                        .text("ratio"),
                );
                ui.add(
                    egui::Slider::new(&mut config.conversion.comp_attack_ms, 1.0..=500.0)
                        .text("attack (ms)"),
                );
                ui.add(
                    egui::Slider::new(&mut config.conversion.comp_release_ms, 50.0..=2000.0)
                        .text("release (ms)"),
                );
                ui.separator();
                ui.label(RichText::new("Bitrate Opus").strong());
                ui.horizontal(|ui| {
                    ui.label("conversion");
                    ui.text_edit_singleline(&mut config.conversion.opus_bitrate);
                    for br in ["12k", "16k", "20k", "32k", "48k", "64k"] {
                        if ui
                            .selectable_label(config.conversion.opus_bitrate == br, br)
                            .clicked()
                        {
                            config.conversion.opus_bitrate = br.into();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("aperçu");
                    ui.text_edit_singleline(&mut config.conversion.opus_preview_bitrate);
                });
            });
        self.show_conversion = open;
    }

    fn warmth(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_warmth;
        egui::Window::new(tr("settings.warmth").trim_end_matches('…').trim_end_matches('.'))
            .id(Id::new("cs_settings_warmth_dsp"))
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Chaîne : HighShelf → Distortion → Compressor → Désourdage → Gain",
                    )
                    .strong()
                    .size(14.0),
                );
                ui.label(
                    RichText::new("Appliqué après DPDF / isolation (pipeline Mixage)")
                        .weak()
                        .small(),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Subtil").clicked() {
                        config.warmth = WarmthParams::preset_subtle();
                    }
                    if ui.button("Standard").clicked() {
                        config.warmth = WarmthParams::preset_standard();
                    }
                    if ui.button("Radio").clicked() {
                        config.warmth = WarmthParams::preset_radio();
                    }
                    if ui.button("Podcast").clicked() {
                        config.warmth = WarmthParams::preset_podcast();
                    }
                });
                ui.separator();
                let w = &mut config.warmth;
                ui.add(
                    egui::Slider::new(&mut w.high_shelf_gain_db, 0.0..=12.0)
                        .text("HighShelf gain (dB)"),
                );
                ui.add(
                    egui::Slider::new(&mut w.high_shelf_cutoff_hz, 100.0..=2000.0)
                        .text("HighShelf cutoff (Hz)"),
                );
                ui.add(
                    egui::Slider::new(&mut w.distortion_drive_db, 0.0..=24.0)
                        .text("Distortion drive (dB)"),
                );
                ui.add(
                    egui::Slider::new(&mut w.compressor_threshold_db, -40.0..=0.0)
                        .text("Compressor threshold (dB)"),
                );
                ui.add(
                    egui::Slider::new(&mut w.compressor_ratio, 1.0..=10.0)
                        .text("Compressor ratio"),
                );
                ui.add(
                    egui::Slider::new(&mut w.final_gain_db, -6.0..=12.0).text("Gain final (dB)"),
                );
                ui.separator();
                ui.checkbox(&mut config.warmth.unmuffle_enabled, tr("mix.unmuffle"));
                let unmuffle_on = config.warmth.unmuffle_enabled;
                ui.add_enabled_ui(unmuffle_on, |ui| {
                    let w = &mut config.warmth;
                    ui.add(
                        egui::Slider::new(&mut w.unmuffle_gain_db, 0.0..=12.0)
                            .text(tr("mix.unmuffle_gain")),
                    );
                    ui.add(
                        egui::Slider::new(&mut w.unmuffle_cutoff_hz, 1500.0..=8000.0)
                            .text(tr("mix.unmuffle_hz")),
                    );
                });
            });
        self.show_warmth = open;
    }

    fn dpdf(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_dpdf;
        config.dpdf.normalize();
        egui::Window::new("DPDF / DPDFNet (nettoyage)")
            .id(Id::new("cs_settings_dpdf_hushmic"))
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Options comme HushMic / Ceva DPDFNet")
                        .strong()
                        .size(14.0),
                );
                ui.label(
                    RichText::new("Vitesse : 16 kHz + Baseline / DPDFNet2 ≫ 48 kHz + DPDFNet8")
                        .weak()
                        .small(),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("Sample rate").strong());
                ui.horizontal(|ui| {
                    for sr in SAMPLE_RATES {
                        if ui
                            .selectable_label(config.dpdf.sample_rate == *sr, *sr)
                            .clicked()
                        {
                            config.dpdf.sample_rate = (*sr).to_string();
                            let labels = model_labels_for_sr(*sr);
                            if !labels.iter().any(|l| *l == config.dpdf.model_label) {
                                config.dpdf.model_label = labels
                                    .iter()
                                    .find(|l| **l == "DPDFNet2")
                                    .or(labels.first())
                                    .copied()
                                    .unwrap_or("DPDFNet2")
                                    .to_string();
                            }
                            config.dpdf.normalize();
                        }
                    }
                });

                ui.add_space(8.0);
                ui.label(RichText::new("Model").strong());
                let labels = model_labels_for_sr(&config.dpdf.sample_rate);
                ui.horizontal_wrapped(|ui| {
                    for label in &labels {
                        if ui
                            .selectable_label(config.dpdf.model_label == *label, *label)
                            .clicked()
                        {
                            config.dpdf.model_label = (*label).to_string();
                            config.dpdf.normalize();
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "ID interne : {} @ {} Hz",
                        config.dpdf.model_id(),
                        config.dpdf.sample_rate_hz()
                    ))
                    .monospace()
                    .small()
                    .weak(),
                );

                ui.separator();
                ui.add(
                    egui::Slider::new(&mut config.dpdf.attn_limit_db, 0.0..=40.0)
                        .text("Limite d'atténuation (dB, 0 = défaut modèle)"),
                );

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "✓ DPDFNet natif (ONNX Runtime) — modèles Ceva téléchargés au 1er usage",
                    )
                    .color(egui::Color32::from_rgb(80, 180, 100)),
                );
            });
        self.show_dpdf = open;
    }

    fn whisper(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_whisper;
        egui::Window::new(tr("settings.whisper"))
            .id(Id::new("cs_settings_whisper"))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("CLI");
                    ui.text_edit_singleline(&mut config.whisper.cli_path);
                    if ui.button(tr("btn.choose")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            config.whisper.cli_path = p.display().to_string();
                        }
                    }
                });
                let discovered = list_discovered_models();
                egui::ComboBox::from_label("model")
                    .selected_text(&config.whisper.model)
                    .show_ui(ui, |ui| {
                        for m in MODEL_NAMES {
                            ui.selectable_value(&mut config.whisper.model, (*m).to_string(), *m);
                        }
                        if !discovered.is_empty() {
                            ui.separator();
                            for m in &discovered {
                                let label = format!("{} ({})", m.name, m.source_label());
                                ui.selectable_value(
                                    &mut config.whisper.model,
                                    m.name.clone(),
                                    label,
                                );
                            }
                        }
                    });
                egui::ComboBox::from_label("language")
                    .selected_text(&config.whisper.language)
                    .show_ui(ui, |ui| {
                        for (id, label) in [
                            ("auto", "auto"),
                            ("fr", "fr"),
                            ("en", "en"),
                            ("de", "de"),
                            ("es", "es"),
                            ("it", "it"),
                            ("french", "french"),
                            ("english", "english"),
                        ] {
                            ui.selectable_value(
                                &mut config.whisper.language,
                                id.to_string(),
                                label,
                            );
                        }
                    });
                egui::ComboBox::from_label("format")
                    .selected_text(config.whisper.format.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config.whisper.format,
                            TranscriptFormat::Txt,
                            "txt",
                        );
                        ui.selectable_value(
                            &mut config.whisper.format,
                            TranscriptFormat::Srt,
                            "srt",
                        );
                        ui.selectable_value(
                            &mut config.whisper.format,
                            TranscriptFormat::Vtt,
                            "vtt",
                        );
                        ui.selectable_value(
                            &mut config.whisper.format,
                            TranscriptFormat::Json,
                            "json",
                        );
                    });
                let status = cs_whisper::detection_status(&config.whisper.cli_path);
                ui.label(RichText::new(status).weak().small());
            });
        self.show_whisper = open;
    }

    fn uvr5(&mut self, ctx: &Context, config: &mut AppConfig) {
        let mut open = self.show_uvr5;
        egui::Window::new(tr("settings.uvr5"))
            .id(Id::new("cs_settings_uvr5"))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("isolate.unavailable"));
                ui.horizontal(|ui| {
                    ui.label("path / CLI");
                    ui.text_edit_singleline(&mut config.uvr5.path);
                    if ui.button(tr("btn.choose")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            config.uvr5.path = p.display().to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("model");
                    ui.text_edit_singleline(&mut config.uvr5.model);
                });
                egui::ComboBox::from_label("stem")
                    .selected_text(&config.uvr5.stem)
                    .show_ui(ui, |ui| {
                        for s in AVAILABLE_STEMS {
                            ui.selectable_value(&mut config.uvr5.stem, (*s).to_string(), *s);
                        }
                    });
                ui.add(egui::Slider::new(&mut config.uvr5.chunk_sec, 60..=3600).text("chunk sec"));
            });
        self.show_uvr5 = open;
    }
}
