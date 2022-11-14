#![warn(clippy::all)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

#[macro_use]
extern crate rust_i18n;
i18n!("locales");

use log::{info, warn};

fn main() {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let current_locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en"));

    info!("Detected locale is {}", current_locale);
    match current_locale.as_str() {
        "en" | "zh-CN" => rust_i18n::set_locale(current_locale.as_str()),
        _ => warn!("Unsoppted locale, fallback to en."),
    }

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        t!("app_name").as_str(),
        native_options,
        Box::new(|cc| Box::new(bs_song_manager::ManagerApp::new(cc))),
    );
}
