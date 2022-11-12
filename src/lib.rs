#![warn(clippy::all)]

mod app;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub use app::ManagerApp;

fn hash_string(data: &Vec<u8>) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

#[derive(Clone)]
enum BeatmapCharacteristic {
    Degree360,
    Degree90,
    Standard,
    NoArrows,
    OneSaber,
    Lawless,
    Lightshow,
}
impl BeatmapCharacteristic {
    fn as_str(&self) -> &'static str {
        match self {
            BeatmapCharacteristic::Degree360 => "360Degree",
            BeatmapCharacteristic::Degree90 => "90Degree",
            BeatmapCharacteristic::Standard => "Standard",
            BeatmapCharacteristic::NoArrows => "NoArrows",
            BeatmapCharacteristic::OneSaber => "OneSaber",
            BeatmapCharacteristic::Lawless => "Lawless",
            BeatmapCharacteristic::Lightshow => "Lightshow",
        }
    }
}

#[derive(Clone)]
struct DifficultyBeatmap {
    difficulty: String,
    difficulty_rank: u64,
    beatmap_filename: String,
}
impl DifficultyBeatmap {
    pub fn new(data: &Value) -> Option<Self> {
        Some(DifficultyBeatmap {
            difficulty: data["_difficulty"].as_str()?.to_string(),
            difficulty_rank: data["_difficultyRank"].as_u64()?,
            beatmap_filename: data["_beatmapFilename"].as_str()?.to_string(),
        })
    }
}

#[derive(Clone)]
struct DifficultyBeatmapSet {
    beatmap_characteristic_name: BeatmapCharacteristic,
    difficulty_beatmaps: Vec<DifficultyBeatmap>,
}
impl DifficultyBeatmapSet {
    pub fn new(data: &Value) -> Option<Self> {
        let mut difficulty_beatmaps = Vec::new();
        let beatmap_characteristic_name = match data["_beatmapCharacteristicName"].as_str()? {
            "360Degree" => BeatmapCharacteristic::Degree360,
            "90Degree" => BeatmapCharacteristic::Degree90,
            "Standard" => BeatmapCharacteristic::Standard,
            "NoArrows" => BeatmapCharacteristic::NoArrows,
            "OneSaber" => BeatmapCharacteristic::OneSaber,
            "Lawless" => BeatmapCharacteristic::Lawless,
            "Lightshow" => BeatmapCharacteristic::Lightshow,
            _ => return None,
        };
        for difficulty_beatmap in data["_difficultyBeatmaps"].as_array()? {
            difficulty_beatmaps.push(DifficultyBeatmap::new(difficulty_beatmap)?)
        }
        Some(DifficultyBeatmapSet {
            beatmap_characteristic_name,
            difficulty_beatmaps,
        })
    }
}
/// This struct should generate from info.dat
///
/// Refer https://github.com/Kylemc1413/SongCore#infodat-explanation
#[derive(Clone)]
pub struct Song {
    song_folder_path: PathBuf,
    song_name: String,
    version: String,
    song_author_name: String,
    level_author_name: String,
    beats_per_minute: u64,
    song_filename: String,
    cover_image_filename: String,
    difficulty_beatmap_sets: Vec<DifficultyBeatmapSet>,
    ///Refer https://github.com/Kylemc1413/SongCore/blob/master/Utilities/Hashing.cs#L173
    level_hash: String,
}

impl Song {
    pub fn from_path(song_path: &PathBuf) -> Option<Self> {
        let file_list = fs::read_dir(song_path);
        let file_list = match file_list {
            Ok(entry) => entry,
            Err(_) => return None,
        };
        let mut hash_data: Vec<u8> = Vec::new();
        for entry in file_list {
            if let Ok(entry) = entry {
                if !entry.path().is_file() || !entry.file_name().eq_ignore_ascii_case("info.dat") {
                    continue;
                }
                let infodat_file = File::open(entry.path());
                let mut infodat_file = match infodat_file {
                    Ok(file) => file,
                    Err(_) => return None,
                };
                let mut buffer = String::new();
                if let Err(_) = infodat_file.read_to_string(&mut buffer) {
                    return None;
                };
                hash_data.extend(buffer.as_bytes());
                let infodat: Result<Value, serde_json::Error> = serde_json::from_str(&buffer);
                let infodat = match infodat {
                    Ok(infodat) => infodat,
                    Err(_) => return None,
                };
                let mut difficulty_beatmap_sets = Vec::new();
                for difficulty_beatmap_set in infodat["_difficultyBeatmapSets"].as_array()? {
                    let data = match DifficultyBeatmapSet::new(difficulty_beatmap_set) {
                        Some(data) => data,
                        None => return None,
                    };
                    for beatmap in &data.difficulty_beatmaps {
                        let mut beatmap_file_path = song_path.clone();
                        beatmap_file_path.push(beatmap.beatmap_filename.clone());
                        let beatmap_file = File::open(beatmap_file_path);
                        let mut beatmap_file = match beatmap_file {
                            Ok(file) => file,
                            Err(_) => return None,
                        };
                        let mut buffer = String::new();
                        if let Err(_) = beatmap_file.read_to_string(&mut buffer) {
                            return None;
                        }
                        hash_data.extend(buffer.as_bytes());
                    }
                    difficulty_beatmap_sets.push(data);
                }
                let result = Song {
                    song_folder_path: song_path.to_path_buf(),
                    song_name: infodat["_songName"].as_str()?.to_string(),
                    version: infodat["_version"].as_str()?.to_string(),
                    song_author_name: infodat["_songAuthorName"].as_str()?.to_string(),
                    level_author_name: infodat["_levelAuthorName"].as_str()?.to_string(),
                    beats_per_minute: infodat["_beatsPerMinute"].as_u64()?,
                    song_filename: infodat["_songFilename"].as_str()?.to_string(),
                    cover_image_filename: infodat["_coverImageFilename"].as_str()?.to_string(),
                    difficulty_beatmap_sets,
                    level_hash: hash_string(&hash_data),
                };
                return Some(result);
            } else {
                continue;
            }
        }
        None
    }
    fn read_cover_image(&self) -> Option<Vec<u8>> {
        let mut cover_image_path = self.song_folder_path.clone();
        cover_image_path.push(&self.cover_image_filename);
        let cover_image_file = File::open(cover_image_path);
        let mut cover_image_file = match cover_image_file {
            Ok(file) => file,
            Err(_) => return None,
        };
        let mut buffer = Vec::new();
        if let Err(_) = cover_image_file.read_to_end(&mut buffer) {
            return None;
        }
        Some(buffer)
    }
}

fn generate_song_list(song_path: &Path) -> Vec<Song> {
    let mut song_list = Vec::new();
    let song_path_entry = fs::read_dir(song_path);
    let song_path_entry = match song_path_entry {
        Ok(entry) => entry,
        Err(_) => return song_list,
    };
    for entry in song_path_entry {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let song_folder_path = entry.path();
        if song_folder_path.is_dir() {
            if let Some(song) = Song::from_path(&song_folder_path) {
                song_list.push(song);
            }
        }
    }
    song_list
}
