#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod player;
mod settings;
mod theme;
mod widgets;

use app::CompressSpeakApp;
use cs_core::APP_NAME;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title(APP_NAME),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(|cc| Ok(Box::new(CompressSpeakApp::new(cc)))),
    )
}
