use std::path::PathBuf;

use egui::{RichText, Vec2};
use egui_extras::RetainedImage;
use rfd::FileDialog;

use crate::{generate_song_list, Song};
fn setup_custom_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();

    // Install my own font (maybe supporting non-latin characters).
    // .ttf and .otf files supported.
    fonts.font_data.insert(
        "source_ttf".to_owned(),
        egui::FontData::from_static(include_bytes!("../fonts/SourceHanSansHW-VF.ttf.ttc")),
    );

    // Put my font first (highest priority) for proportional text:
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "source_ttf".to_owned());

    // Put my font as last fallback for monospace:
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("source_ttf".to_owned());

    // Tell egui to use these fonts:
    ctx.set_fonts(fonts);
}

pub struct ManagerApp {
    song_folder: PathBuf,
    song_list: Vec<Song>,
    current_song: Option<Song>,
}

impl Default for ManagerApp {
    fn default() -> Self {
        Self {
            song_folder: std::env::current_dir().unwrap(),
            song_list: Vec::new(),
            current_song: None,
        }
    }
}

impl ManagerApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customized the look at feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        setup_custom_fonts(&cc.egui_ctx);
        Default::default()
    }
}

impl eframe::App for ManagerApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Self {
            song_folder,
            song_list,
            current_song,
        } = self;

        // Examples of how to create different panels and windows.
        // Pick whichever suits you.
        // Tip: a good default choice is to just keep the `CentralPanel`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                if ui.button("Open").clicked() {
                    let select_dir = FileDialog::new().pick_folder();
                    if select_dir.is_some() {
                        *song_folder = select_dir.unwrap();
                        *song_list = generate_song_list(song_folder);
                    }
                }
                ui.label("Current Working Path: ");
                ui.label(&(*song_folder.as_path().display().to_string()));
            });
        });

        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Song List");

            let text = format!("Total {} songs.", song_list.len());
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        for song in song_list {
                            ui.separator();
                            if ui.link(&song.song_name).clicked() {
                                *current_song = Some(song.clone());
                            }
                        }
                        ui.separator();
                    })
                });
            ui.heading(text);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(current_song) = current_song {
                ui.heading(&current_song.song_name);
                ui.label(RichText::new(format!("version: {}", &current_song.version)).size(12.0));
                ui.end_row();
                ui.label(format!("Level Hash: {}", &current_song.level_hash));
                ui.label(format!("Song Author: {}", &current_song.song_author_name));
                ui.label(format!("Level Author: {}", &current_song.level_author_name));
                ui.label(format!("BPM: {}", &current_song.beats_per_minute));
                ui.end_row();
                if let Some(image) = current_song.read_cover_image() {
                    let image = RetainedImage::from_image_bytes("cover", &image[..]).unwrap();
                    ui.add(egui::Image::new(
                        image.texture_id(ctx),
                        Vec2::new(256.0, 256.0),
                    ));
                }
                ui.end_row();
            } else {
                ui.heading("No selected song.");
            }

            egui::warn_if_debug_build(ui);
        });
    }
}
