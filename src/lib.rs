#[macro_use]
extern crate rust_i18n;
i18n!("locales");

mod app;
pub use app::ManagerApp;
use deunicode::deunicode;

use log::{debug, error, info, warn};
use native_tls::{TlsConnector, TlsStream};
use regex::Regex;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::VecDeque;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::RwLock;
use std::thread;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

static CONCURRENT_THREADS_MAX: usize = 16;
static CONCURRENT_THREADS_MIN: usize = 8;
static DEFAULT_ID: &str = "00000";
static BEATSAVER_DOMAIN: &str = "api.beatsaver.com";
static BEATSAVER_ADDR: &str = "api.beatsaver.com:443";

fn get_api_connection() -> Result<TlsStream<TcpStream>, Box<dyn std::error::Error>> {
    let connector = TlsConnector::new().unwrap();
    debug!("Connecting to {}...", BEATSAVER_ADDR);
    let stream = TcpStream::connect(BEATSAVER_ADDR)?;
    debug!("Connected to {}.", BEATSAVER_ADDR);
    let stream = connector.connect(BEATSAVER_DOMAIN, stream)?;
    Ok(stream)
}

fn get_id_by_hash(hash: &str) -> String {
    debug!("Trying to get id by hash {}", hash);
    let default_id = String::from(DEFAULT_ID);
    let request = format!(
        "GET /maps/hash/{} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\n\r\n",
        hash, BEATSAVER_DOMAIN
    );
    debug!("Trying to get connection to api server.");
    let mut stream = match get_api_connection() {
        Ok(stream) => stream,
        Err(error) => {
            warn!("Get id for hash {} failed.{}", hash, error);
            return default_id;
        }
    };
    debug!("Get connection successful.");
    debug!("Sending request...\n{}", request);
    if let Err(error) = stream.write_all(request.as_bytes()) {
        warn!("Failed to send request to api server.{}", error);
        return default_id;
    }
    debug!("Send request done.");
    let mut reader = BufReader::new(stream);
    let mut bytes_to_read: usize = 0;
    loop {
        let mut buf = vec![];
        if let Err(error) = reader.read_until(b'\n', &mut buf) {
            warn!("Got error when reading http head.{}", error);
            break;
        }
        let head = String::from_utf8_lossy(&buf);
        debug!("Read head from server: {}", head);
        if head.starts_with("Content-Length:") {
            bytes_to_read = match head.split(": ").nth(1) {
                Some(str) => match str.trim().parse::<usize>() {
                    Ok(size) => size,
                    Err(error) => {
                        warn!("Failed to parse Content-Length.{}", error);
                        return default_id;
                    }
                },
                None => {
                    warn!("Failed to parse Content-Length.");
                    return default_id;
                }
            };
            debug!("bytes_to_read={}", bytes_to_read);
        }
        if head.trim().is_empty() {
            break;
        }
    }
    let mut resp = vec![0u8; bytes_to_read];
    if let Err(error) = reader.read_exact(&mut resp) {
        warn!("Got error when reading http body.{}", error);
        return default_id;
    }
    let body = String::from_utf8_lossy(&resp);
    debug!("API server return response.\n{}", body);

    let content: Value = match serde_json::from_str(body.as_ref()) {
        Ok(content) => content,
        Err(error) => {
            warn!("Failed to parse json.{}", error);
            return default_id;
        }
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
    pub fn from_path(
        song_path: &PathBuf,
        id_cache: &Arc<RwLock<HashMap<String, String>>>,
    ) -> Option<Self> {
        let file_list = read_dir(song_path);
        let file_list = match file_list {
            Ok(entry) => entry,
            Err(error) => {
                error!("Read file list failed. {}", error);
                return None;
            }
        };
        let mut hash_data: Vec<u8> = Vec::new();
        for entry in file_list.flatten() {
            if !entry.path().is_file() || !entry.file_name().eq_ignore_ascii_case("info.dat") {
                continue;
            }
            let infodat_file = File::open(entry.path());
            let mut infodat_file = match infodat_file {
                Ok(file) => file,
                Err(error) => {
                    error!("Read info.dat failed. {}", error);
                    return None;
                }
            };
            let mut buffer = String::new();
            if let Err(error) = infodat_file.read_to_string(&mut buffer) {
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
                    let beatmap_file = File::open(beatmap_file_path);
                    let mut beatmap_file = match beatmap_file {
                        Ok(file) => file,
                        Err(error) => {
                            error!("Read beatmap file failed. {}", error);
                            return None;
                        }
                    };
                    let mut buffer = String::new();
                    if let Err(error) = beatmap_file.read_to_string(&mut buffer) {
                        error!("Read beatmap file failed. {}", error);
                        return None;
                    }
                    hash_data.extend(buffer.as_bytes());
                }
                difficulty_beatmap_sets.push(data);
            }
            let level_hash = hash_string(&hash_data);
            let level_id = match id_cache.write() {
                Ok(mut id_cache) => match id_cache.get(&level_hash) {
                    Some(id) => id.clone(),
                    None => {
                        let id = get_id_by_hash(level_hash.as_str());
                        if id != DEFAULT_ID {
                            id_cache.insert(level_hash.clone(), id.clone());
                        }
                        id
                    }
                },
                Err(error) => {
                    warn!("Failed to get cache lock.{}", error);
                    get_id_by_hash(level_hash.as_str())
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
        let name = deunicode(self.song_name.as_str());
        let author = deunicode(self.level_author_name.as_str());
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
    let song_path_entry = read_dir(song_path);
    let song_path_entry = match song_path_entry {
        Ok(entry) => entry,
        Err(error) => {
            error!("Load song path failed. {}", error);
            return (song_list, invalid_path);
        }
    };
    let shared_song_list = Arc::new(RwLock::new(Vec::new()));
    let shared_invalid_path = Arc::new(RwLock::new(HashSet::new()));
    let cached_id = Arc::new(RwLock::new(HashMap::new()));
    let mut task_list = Vec::new();

    let mut cache_id_file = PathBuf::new();
    cache_id_file.push(song_path);
    cache_id_file.push("id.cache");
    match std::fs::File::open(cache_id_file.as_path()) {
        Ok(cache_id_file) => {
            match serde_json::from_reader(cache_id_file) {
                Ok(data) => {
                    *cached_id.write().unwrap() = data;
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
            let cache_id_cloned = cached_id.clone();
            let shared_song_list_cloned = shared_song_list.clone();
            let shared_invalid_path_cloned = shared_invalid_path.clone();
            let task = move || {
                debug!(
                    "Loading song from {}.",
                    &song_folder_path.as_path().display()
                );
                if let Some(song) = Song::from_path(&song_folder_path, &cache_id_cloned) {
                    shared_song_list_cloned.write().unwrap().push(song);
                } else {
                    shared_invalid_path_cloned
                        .write()
                        .unwrap()
                        .insert(song_folder_path);
                }
            };
            task_list.push(task);
        } else if !song_folder_path.ends_with("id.cache") {
            warn!(
                "Entry {} is not a directory.",
                song_folder_path.as_path().display()
            );
            invalid_path.insert(song_folder_path);
        }
    }

    let mut task_pending = VecDeque::new();
    for task in task_list {
        if task_pending.len() < CONCURRENT_THREADS_MAX {
            let task = thread::spawn(task);
            task_pending.push_back(task);
        } else {
            while task_pending.len() > CONCURRENT_THREADS_MIN {
                task_pending.pop_front().unwrap().join().unwrap();
            }
        }
    }
    if !task_pending.is_empty() {
        for task in task_pending {
            task.join().unwrap();
        }
    }

    song_list = shared_song_list.read().unwrap().clone();
    song_list.sort_by(|a, b| a.song_name.cmp(&b.song_name));
    invalid_path.extend(shared_invalid_path.read().unwrap().clone());

    match std::fs::File::create(cache_id_file.as_path()) {
        Ok(cache_id_file) => {
            let id_cache = &*cached_id.read().unwrap();
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
    Delete,
    Rename,
}

impl Action {
    fn as_str(&self) -> &'static str {
        match self {
            Action::Delete => "Delete",
            Action::Rename => "Rename",
        }
    }
}

fn apply_changes(pending_changes: &HashMap<Song, Action>) {
    for (song, action) in pending_changes {
        if let Err(error) = match action {
            Action::Delete => {
                info!("Deleting {}", song.song_folder_path.as_path().display());
                std::fs::remove_dir_all(song.song_folder_path.as_path())
            }
            Action::Rename => {
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
