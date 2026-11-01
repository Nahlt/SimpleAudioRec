//! SimpleAudioRec — registra l'audio del PC (output + microfono) durante i meeting.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod hotkeys;
mod mute_worker;
mod platform;
mod ui;
#[cfg(windows)]
mod wasapi;

use simpleaudiorec_core::audio::AudioBackend;
use simpleaudiorec_core::paths::DataPaths;
use simpleaudiorec_core::settings::Settings;
use simpleaudiorec_core::APP_NAME;
use std::sync::Arc;

fn backend() -> Arc<dyn AudioBackend> {
    #[cfg(windows)]
    {
        Arc::new(wasapi::WasapiBackend)
    }
    #[cfg(not(windows))]
    {
        Arc::new(simpleaudiorec_core::audio::NullBackend)
    }
}

fn main() -> eframe::Result {
    // Istanza singola: se è già aperta, la porta in primo piano ed esce.
    let Some(single) = platform::single_instance() else { return Ok(()) };

    let paths = DataPaths::detect();
    let (settings, settings_error) = Settings::load(&paths.settings_file());

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-64.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_app_id(APP_NAME)
        .with_inner_size([420.0, 250.0])
        .with_resizable(false)
        .with_maximize_button(false);
    if let Some(i) = icon {
        viewport = viewport.with_icon(i);
    }
    if let Some([x, y]) = settings.window_pos
        && platform::point_on_screen(x, y)
    {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: settings.window_pos.is_none(),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::SimpleAudioRecApp::new(cc, paths, settings, settings_error, backend(), Some(single))))
        }),
    )
}
