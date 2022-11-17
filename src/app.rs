use egui::Vec2;
use egui_extras::{RetainedImage, Size, TableBuilder};
use log::warn;
use rfd::FileDialog;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use rust_i18n::t;
use std::io::BufReader;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::{apply_changes, generate_song_list, Action, Song};
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
    list_outdated: bool,
    invalid_path: HashSet<PathBuf>,
    pending_changes: HashMap<Song, Action>,
    current_song: Option<Song>,
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
}

impl Default for ManagerApp {
    fn default() -> Self {
        let (_stream, stream_handle) = match OutputStream::try_default() {
            Ok((_stream, stream_handle)) => (Some(_stream), Some(stream_handle)),
            Err(error) => {
                warn!("Init audio failed.{}", error);
                (None, None)
            }
        };
        Self {
            song_folder: std::env::current_dir().unwrap(),
            song_list: Vec::new(),
            list_outdated: false,
            invalid_path: HashSet::new(),
            pending_changes: HashMap::new(),
            current_song: None,
            _stream,
            stream_handle,
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
            list_outdated,
            invalid_path,
            pending_changes,
            current_song,
            _stream,
            stream_handle,
            sink,
        } = self;

        if *list_outdated {
            (*song_list, *invalid_path) = generate_song_list(&song_folder);
            *list_outdated = false;
        }

        egui::TopBottomPanel::top("menu_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button(t!("ui.open_song_folder")).clicked() {
                    let select_dir = FileDialog::new().pick_folder();
                    if let Some(select_dir) = select_dir {
                        *song_folder = select_dir;
                        *list_outdated = true;
                    }
                }
                ui.label(t!("ui.current_working_folder"));
                ui.label(&(*song_folder.as_path().display().to_string()));
            });
        });

        egui::SidePanel::left("song_list_panel").show(ctx, |ui| {
            ui.heading(t!("ui.song_list_title"));

            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        for song in song_list {
                            if ui.link(&song.song_name).clicked() {
                                *current_song = Some(song.clone());
                            }
                            ui.separator();
                        }
                    })
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(current_song) = current_song {
                ui.heading(&current_song.song_name);
                ui.label(&current_song.song_sub_name);
                ui.end_row();
                ui.separator();
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
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(t!("ui.level_id", id = &current_song.level_id));
                    if ui
                        .button("📋")
                        .on_hover_text(t!("ui.click_to_copy"))
                        .clicked()
                    {
                        ui.output().copied_text = current_song.level_id.to_string();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(t!("ui.level_hash", hash = &current_song.level_hash));
                    if ui
                        .button("📋")
                        .on_hover_text(t!("ui.click_to_copy"))
                        .clicked()
                    {
                        ui.output().copied_text = current_song.level_hash.to_string();
                    }
                });
                ui.separator();
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
                }
            } else {
                ui.heading(t!("ui.no_song_hint"));
            }
            egui::warn_if_debug_build(ui);
        });

        egui::SidePanel::right("pending_change_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(t!("ui.pending_change_list_title"));
                ui.separator();
                if ui.button(t!("ui.commit_changes")).clicked() {
                    if !pending_changes.is_empty() {
                        apply_changes(pending_changes);
                        *pending_changes = HashMap::new();
                        *current_song = None;
                        *list_outdated = true;
                    }
                }
                if ui.button(t!("ui.reset_changes")).clicked() {
                    *pending_changes = HashMap::new();
                }
            });
            if !pending_changes.is_empty() {
                let mut withdraw_list = HashMap::new();
                TableBuilder::new(ui)
                    .column(Size::exact(40.0))
                    .column(Size::remainder().at_least(40.0))
                    .column(Size::exact(10.0))
                    .header(20.0, |mut header| {
                        header.col(|ui| {
                            ui.heading(t!("ui.pending_action_title"));
                        });
                        header.col(|ui| {
                            ui.heading(t!("ui.pending_song_title"));
                        });
                        header.col(|ui| {
                            ui.heading("");
                        });
                    })
                    .body(|mut body| {
                        for (song, action) in pending_changes.clone() {
                            body.row(30.0, |mut row| {
                                row.col(|ui| {
                                    ui.label(match action {
                                        Action::DELETE => t!("ui.delete"),
                                        Action::RENAME => t!("ui.rename"),
                                    });
                                });
                                row.col(|ui| {
                                    ui.label(song.song_name.as_str()).on_hover_text(
                                        song.song_folder_path.as_path().display().to_string(),
                                    );
                                });
                                row.col(|ui| {
                                    if ui.button("-").clicked() {
                                        withdraw_list.insert(song, action);
                                    }
                                });
                            });
                        }
                    });
                if !withdraw_list.is_empty() {
                    for (k, _v) in withdraw_list {
                        pending_changes.remove(&k);
                    }
                }
            }
        });

        egui::TopBottomPanel::bottom("action_panel").show(ctx, |ui| {
            if let Some(song) = current_song {
                let rename_tip = format!(
                    "{}\n⬇\n{}",
                    song.song_folder_path.file_name().unwrap().to_str().unwrap(),
                    song.get_canonical_name()
                );
                ui.horizontal(|ui| {
                    if ui.button(t!("ui.delete")).clicked() {
                        pending_changes.insert(song.clone(), Action::DELETE);
                    }
                    if ui
                        .button(t!("ui.rename"))
                        .on_hover_text(rename_tip)
                        .clicked()
                    {
                        pending_changes.insert(song.clone(), Action::RENAME);
                    }
                });
                if let Some(stream_handle) = stream_handle {
                    ui.horizontal(|ui| {
                        if ui.button("▶").clicked() {
                            if let Some(file) = song.read_song_file() {
                                match stream_handle.play_once(BufReader::new(file)) {
                                    Ok(play_sink) => *sink = Some(play_sink),
                                    Err(error) => {
                                        warn!("play error {}", error);
                                    }
                                }
                            }
                        }
                        if ui.button("■").clicked() {
                            if let Some(sink) = sink {
                                sink.stop();
                            }
                            *sink = None;
                        }
                    });
                }
            }
        });
    }
}
