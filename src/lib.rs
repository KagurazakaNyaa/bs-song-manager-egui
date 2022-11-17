#![warn(clippy::all)]
#[macro_use]
extern crate rust_i18n;
i18n!("locales");

mod app;
pub use app::ManagerApp;
use deunicode::deunicode;

use hyper::{body::Buf, Body, Request};
use log::{debug, error, info, warn};
use regex::Regex;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

static CONCURRENT_THREADS: usize = 16;
static DEFAULT_ID: &str = "00000";
static BEATSAVER_DOMAIN: &str = "api.beatsaver.com";
static BEATSAVER_ADDR: &str = "api.beatsaver.com:443";

async fn get_api_connection() -> Result<TlsStream<TcpStream>, Box<dyn std::error::Error>> {
    debug!("Connecting to {}...", BEATSAVER_ADDR);
    let socket = TcpStream::connect(BEATSAVER_ADDR).await?; //TODO fix hang
    debug!("Connected to {}.", BEATSAVER_ADDR);
    let cx = match native_tls::TlsConnector::builder().build() {
        Ok(cx) => cx,
        Err(_) => todo!(),
    };
    let cx = tokio_native_tls::TlsConnector::from(cx);
    let stream = cx.connect(BEATSAVER_DOMAIN, socket).await?;
    Ok(stream)
}

async fn get_id_by_hash(hash: &str) -> String {
    debug!("Trying to get id by hash {}", hash);
    let default_id = String::from(DEFAULT_ID);
    let req = match Request::builder()
        .uri(format!("/maps/hash/{}", hash))
        .header(hyper::header::HOST, BEATSAVER_DOMAIN)
        .body(Body::empty())
    {
        Ok(req) => req,
        Err(_) => todo!(),
    };
    debug!("Trying to get connection to api server.");
    let stream = match get_api_connection().await {
        Ok(stream) => stream,
        Err(error) => {
            warn!("Get id for hash {} failed.{}", hash, error);
            return default_id;
        }
    };
    debug!("Get connection successful.");
    let (mut sender, _conn) = match hyper::client::conn::handshake(stream).await {
        Ok((sender, conn)) => (sender, conn),
        Err(_) => todo!(),
    };
    let body = match sender.send_request(req).await {
        Ok(res) => match hyper::body::aggregate(res).await {
            Ok(body) => body,
            Err(_) => todo!(),
        },
        Err(_) => todo!(),
    };

    let content: Value = match serde_json::from_reader(body.reader()) {
        Ok(content) => content,
        Err(_) => todo!(),
    };

    let id = match content["id"].as_str() {
        Some(id) => id.to_string(),
        None => default_id,
    };
    debug!("Got level id {} for hash {}", id, hash);
    id
}

fn hash_string(data: &Vec<u8>) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, PartialEq, Eq)]
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
#[derive(Clone, Eq)]
pub struct Song {
    song_folder_path: PathBuf,
    song_name: String,
    song_sub_name: String,
    song_author_name: String,
    level_author_name: String,
    beats_per_minute: u64,
    song_filename: String,
    cover_image_filename: String,
    difficulty_beatmap_sets: Vec<DifficultyBeatmapSet>,
    ///Refer https://github.com/Kylemc1413/SongCore/blob/master/Utilities/Hashing.cs#L173
    level_hash: String,
    level_id: String,
}

impl PartialEq for Song {
    fn eq(&self, other: &Self) -> bool {
        self.song_folder_path == other.song_folder_path
    }
}
impl Hash for Song {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.song_folder_path.hash(state);
    }
}

impl Song {
    pub async fn from_path(
        song_path: &PathBuf,
        id_cache: &Arc<Mutex<HashMap<String, String>>>,
    ) -> Option<Self> {
        let file_list = tokio::fs::read_dir(song_path).await;
        let mut file_list = match file_list {
            Ok(entry) => entry,
            Err(error) => {
                error!("Read file list failed. {}", error);
                return None;
            }
        };
        let mut hash_data: Vec<u8> = Vec::new();
        loop {
            let entry = file_list.next_entry().await;
            let entry = match entry {
                Ok(entry) => match entry {
                    Some(entry) => entry,
                    None => {
                        break;
                    }
                },
                Err(error) => {
                    error!("Read file list failed. {}", error);
                    return None;
                }
            };
            if !entry.path().is_file() || !entry.file_name().eq_ignore_ascii_case("info.dat") {
                continue;
            }
            let infodat_file = tokio::fs::File::open(entry.path()).await;
            let mut infodat_file = match infodat_file {
                Ok(file) => file,
                Err(error) => {
                    error!("Read info.dat failed. {}", error);
                    return None;
                }
            };
            let mut buffer = String::new();
            if let Err(error) = infodat_file.read_to_string(&mut buffer).await {
                error!("Read info.dat failed. {}", error);
                return None;
            };
            hash_data.extend(buffer.as_bytes());
            let infodat: Result<Value, serde_json::Error> = serde_json::from_str(&buffer);
            let infodat = match infodat {
                Ok(infodat) => infodat,
                Err(error) => {
                    error!("Read info.dat failed. {}", error);
                    return None;
                }
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
                    let beatmap_file = tokio::fs::File::open(beatmap_file_path).await;
                    let mut beatmap_file = match beatmap_file {
                        Ok(file) => file,
                        Err(error) => {
                            error!("Read beatmap file failed. {}", error);
                            return None;
                        }
                    };
                    let mut buffer = String::new();
                    if let Err(error) = beatmap_file.read_to_string(&mut buffer).await {
                        error!("Read beatmap file failed. {}", error);
                        return None;
                    }
                    hash_data.extend(buffer.as_bytes());
                }
                difficulty_beatmap_sets.push(data);
            }
            let level_hash = hash_string(&hash_data);
            let level_id = match id_cache.lock() {
                Ok(mut id_cache) => match id_cache.get(&level_hash) {
                    Some(id) => id.clone(),
                    None => {
                        let id = get_id_by_hash(level_hash.as_str()).await;
                        if id != DEFAULT_ID {
                            id_cache.insert(level_hash.clone(), id.clone());
                        }
                        id
                    }
                },
                Err(error) => {
                    warn!("Failed to get cache lock.{}", error);
                    get_id_by_hash(level_hash.as_str()).await
                }
            };
            let result = Song {
                song_folder_path: song_path.to_path_buf(),
                song_name: infodat["_songName"].as_str()?.to_string(),
                song_sub_name: infodat["_songSubName"].as_str()?.to_string(),
                song_author_name: infodat["_songAuthorName"].as_str()?.to_string(),
                level_author_name: infodat["_levelAuthorName"].as_str()?.to_string(),
                beats_per_minute: infodat["_beatsPerMinute"].as_u64()?,
                song_filename: infodat["_songFilename"].as_str()?.to_string(),
                cover_image_filename: infodat["_coverImageFilename"].as_str()?.to_string(),
                difficulty_beatmap_sets,
                level_hash,
                level_id,
            };
            return Some(result);
        }
        None
    }
    fn read_cover_image(&self) -> Option<Vec<u8>> {
        let mut cover_image_path = self.song_folder_path.clone();
        cover_image_path.push(&self.cover_image_filename);
        let cover_image_file = std::fs::File::open(cover_image_path);
        let mut cover_image_file = match cover_image_file {
            Ok(file) => file,
            Err(error) => {
                error!("Load cover failed. {}", error);
                return None;
            }
        };
        let mut buffer = Vec::new();
        if let Err(error) = cover_image_file.read_to_end(&mut buffer) {
            error!("Load cover failed. {}", error);
            return None;
        }
        Some(buffer)
    }
    fn read_song_file(&self) -> Option<std::fs::File> {
        let mut song_file_path = self.song_folder_path.clone();
        song_file_path.push(&self.song_filename);
        info!("Load sound failed. {}", &song_file_path.as_path().display());
        match std::fs::File::open(song_file_path) {
            Ok(song_file) => Some(song_file),
            Err(error) => {
                error!("Load sound failed. {}", error);
                None
            }
        }
    }
    /// The canonical naming of the folder refers to the naming method of the song package shared by WGzeyu(https://bs.wgzeyu.com/).
    fn get_canonical_name(&self) -> String {
        let name = deunicode(&self.song_name.as_str());
        let author = deunicode(&self.level_author_name.as_str());
        let regex = Regex::new(r#"[~#"%&*:<>?/\\{|}]+"#).unwrap();
        regex
            .replace_all(
                format!("{} ({} - {})", &self.level_id, name, author).as_str(),
                "_",
            )
            .to_string()
    }
}

fn generate_song_list(song_path: &Path) -> (Vec<Song>, HashSet<PathBuf>) {
    let mut song_list = Vec::new();
    let mut invalid_path = HashSet::new();
    let song_path_entry = std::fs::read_dir(song_path);
    let song_path_entry = match song_path_entry {
        Ok(entry) => entry,
        Err(error) => {
            error!("Load song path failed. {}", error);
            return (song_list, invalid_path);
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            error!("Init tokio runtime failed. {}", error);
            return (song_list, invalid_path);
        }
    };
    let shared_song_list = Arc::new(Mutex::new(Vec::new()));
    let shared_invalid_path = Arc::new(Mutex::new(HashSet::new()));
    let cached_id = Arc::new(Mutex::new(HashMap::new()));
    let mut future_list = Vec::new();

    let mut cache_id_file = PathBuf::new();
    cache_id_file.push(song_path.clone());
    cache_id_file.push("id.cache");
    match std::fs::File::open(cache_id_file.as_path()) {
        Ok(cache_id_file) => {
            match serde_json::from_reader(cache_id_file) {
                Ok(data) => {
                    *cached_id.lock().unwrap() = data;
                }
                Err(error) => {
                    warn!("Parse id cache failed. {}", error);
                }
            };
        }
        Err(error) => {
            warn!("Load id cache failed.{}", error);
        }
    };

    for entry in song_path_entry {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warn!("Some entry read failed.{}", error);
                continue;
            }
        };
        let song_folder_path = entry.path();
        if song_folder_path.is_dir() {
            let task = async {
                debug!(
                    "Loading song from {}.",
                    &song_folder_path.as_path().display()
                );
                if let Some(song) = Song::from_path(&song_folder_path, &cached_id).await {
                    shared_song_list.lock().unwrap().push(song);
                } else {
                    shared_invalid_path.lock().unwrap().insert(song_folder_path);
                }
            };
            future_list.push(task);
        } else if !song_folder_path.ends_with("id.cache") {
            warn!(
                "Entry {} is not a directory.",
                song_folder_path.as_path().display()
            );
            invalid_path.insert(song_folder_path);
        }
    }

    while !future_list.is_empty() {
        let mut task_list = Vec::new();
        while task_list.len() < CONCURRENT_THREADS {
            match future_list.pop() {
                Some(task) => task_list.push(task),
                None => break,
            };
        }
        let all_task = futures::future::join_all(task_list);
        runtime.block_on(all_task);
    }

    song_list = shared_song_list.lock().unwrap().clone();
    song_list.sort_by(|a, b| a.song_name.cmp(&b.song_name));
    invalid_path.extend(shared_invalid_path.lock().unwrap().clone());

    match std::fs::File::create(cache_id_file.as_path()) {
        Ok(cache_id_file) => {
            let id_cache = &*cached_id.lock().unwrap();
            if let Err(error) = serde_json::to_writer(cache_id_file, id_cache) {
                warn!("Save id cache failed.{}", error);
            }
        }
        Err(error) => {
            warn!("Save id cache failed.{}", error);
        }
    };
    (song_list, invalid_path)
}

#[derive(Clone, PartialEq, Eq)]
enum Action {
    DELETE,
    RENAME,
}

impl Action {
    fn as_str(&self) -> &'static str {
        match self {
            Action::DELETE => "Delete",
            Action::RENAME => "Rename",
        }
    }
}

fn apply_changes(pending_changes: &HashMap<Song, Action>) {
    for (song, action) in pending_changes {
        if let Err(error) = match action {
            Action::DELETE => {
                info!("Deleting {}", song.song_folder_path.as_path().display());
                std::fs::remove_dir_all(song.song_folder_path.as_path())
            }
            Action::RENAME => {
                if let Some(dst) = song.song_folder_path.parent() {
                    let mut dst = PathBuf::from(dst);
                    dst.push(song.get_canonical_name());
                    info!(
                        "Renaming {} to {}",
                        song.song_folder_path.as_path().display(),
                        dst.as_path().display()
                    );
                    std::fs::rename(song.song_folder_path.as_path(), dst)
                } else {
                    warn!("Path {} invalid", song.song_folder_path.as_path().display());
                    continue;
                }
            }
        } {
            warn!(
                "Failed to {} {}.{}",
                action.as_str(),
                song.song_folder_path.as_path().display(),
                error
            );
        }
    }
}
