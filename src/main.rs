#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod mpv;
mod notes;
mod timecode;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::APP_TITLE)
            .with_inner_size([1280.0, 780.0])
            .with_maximized(true)
            .with_min_inner_size([820.0, 520.0])
            .with_drag_and_drop(true)
            .with_icon(egui::IconData { rgba: include_bytes!("../assets/icon-64.rgba").to_vec(), width: 64, height: 64 }),
        ..Default::default()
    };
    eframe::run_native("ScrubNotes", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial)))))
}
