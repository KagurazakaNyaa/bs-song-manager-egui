use egui::Vec2;
use egui_extras::RetainedImage;
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, Sink};
use rust_i18n::t;
use std::{io::BufReader, path::PathBuf};

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
    sink: Option<Sink>,
}

impl Default for ManagerApp {
    fn default() -> Self {
        Self {
            song_folder: std::env::current_dir().unwrap(),
            song_list: Vec::new(),
            current_song: None,
            sink: None,
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
            sink,
        } = self;

        // Examples of how to create different panels and windows.
        // Pick whichever suits you.
        // Tip: a good default choice is to just keep the `CentralPanel`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                if ui.button(t!("ui.open_song_folder")).clicked() {
                    let select_dir = FileDialog::new().pick_folder();
                    if let Some(select_dir) = select_dir {
                        *song_folder = select_dir;
                        *song_list = generate_song_list(song_folder);
                        if let Some(sink) = sink {
                            sink.stop();
                        }
                        *sink = None;
                    }
                }
                ui.label(t!("ui.current_working_folder"));
                ui.label(&(*song_folder.as_path().display().to_string()));
            });
        });

        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading(t!("ui.song_list_title"));

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        for song in song_list {
                            ui.separator();
                            if ui.link(&song.song_name).clicked() {
                                *current_song = Some(song.clone());
                                if let Some(sink) = sink {
                                    sink.stop();
                                }
                                *sink = None;
                            }
                        }
                        ui.separator();
                    })
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(current_song) = current_song {
                ui.heading(&current_song.song_name);
                ui.end_row();
                ui.label(t!(
                    "ui.song_author",
                    author = &current_song.song_author_name
                ));
                ui.label(t!(
                    "ui.level_author",
                    author = &current_song.level_author_name
                ));
                ui.label(t!(
                    "ui.bpm",
                    bpm = &current_song.beats_per_minute.to_string()
                ));
                ui.end_row();
                if let Some(image) = current_song.read_cover_image() {
                    let image = RetainedImage::from_image_bytes("cover", &image[..]).unwrap();
                    ui.add(egui::Image::new(
                        image.texture_id(ctx),
                        Vec2::new(256.0, 256.0),
                    ));
                }
                ui.end_row();
                ui.separator();
                for difficulty_beatmap_set in &current_song.difficulty_beatmap_sets {
                    ui.horizontal_wrapped(|ui| {
                        ui.collapsing(
                            difficulty_beatmap_set.beatmap_characteristic_name.as_str(),
                            |ui| {
                                for difficulty_beatmap in
                                    &difficulty_beatmap_set.difficulty_beatmaps
                                {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        ui.label(&difficulty_beatmap.difficulty).on_hover_text(t!(
                                            "ui.difficulty_rank",
                                            rank = &difficulty_beatmap.difficulty_rank.to_string()
                                        ));
                                    });
                                }
                            },
                        );
                    });
                    ui.separator();
                    ui.label(t!("ui.level_hash", hash = &current_song.level_hash));
                    if ui
                        .button("📋")
                        .on_hover_text(t!("ui.click_to_copy"))
                        .clicked()
                    {
                        ui.output().copied_text = current_song.level_hash.to_string();
                    }
                    ui.separator();
                }
            } else {
                ui.heading(t!("ui.no_song_hint"));
            }
            egui::warn_if_debug_build(ui);
        });
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            if let Some(song) = current_song {
                if ui.button(t!("ui.play")).clicked() {
                    if let Some(song_file) = song.read_song_file() {
                        if let Ok(source) = Decoder::new_vorbis(BufReader::new(song_file)) {
                            if let Some(sink) = sink {
                                sink.append(source);
                                sink.play();
                            } else {
                                let (_stream, stream_handle) = OutputStream::try_default().unwrap();
                                let newsink = Sink::try_new(&stream_handle).unwrap();
                                newsink.append(source);
                                newsink.play();
                                *sink = Some(newsink);
                            }
                        }
                    }
                }
                if ui.button(t!("ui.stop")).clicked() {
                    if let Some(sink) = sink {
                        sink.stop();
                    }
                    *sink = None;
                }
            }
        });
    }
}
