use mz_module_support::{self as support, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    path: String,
    recursive: bool,
    hidden: bool,
    shuffle: bool,
    scan_limit: usize,
    probe_timeout: u64,
    ffmpeg: String,
    ffprobe: String,
    fluidsynth: String,
    soundfont: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            path: String::new(),
            recursive: true,
            hidden: false,
            shuffle: false,
            scan_limit: 10000,
            probe_timeout: 5,
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            fluidsynth: "fluidsynth".into(),
            soundfont: String::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    id: String,
    path: PathBuf,
    size: u64,
    modified: u128,
    title: String,
    artist: String,
    duration_ms: i64,
}
#[derive(Default, Serialize, Deserialize)]
struct Index {
    root: PathBuf,
    entries: Vec<Entry>,
}
fn index_path() -> PathBuf {
    support::paths::cache_dir("mz-local").join("index.json")
}
fn root(settings: &Settings) -> Result<PathBuf> {
    if settings.path.is_empty() {
        return Err("Укажите папку: mz set local path /путь/к/музыке".into());
    }
    let path = std::fs::canonicalize(&settings.path)?;
    if !path.is_dir() {
        return Err("path must be a directory".into());
    }
    Ok(path)
}
fn files(root: &Path, settings: &Settings) -> Result<Vec<PathBuf>> {
    let mut dirs = vec![root.to_owned()];
    let mut found = Vec::new();
    let mut examined = 0;
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            examined += 1;
            if examined > settings.scan_limit {
                return Err("Лимит сканирования превышен; увеличьте scan_limit".into());
            }
            if !settings.hidden && entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() && settings.recursive {
                dirs.push(entry.path());
            } else if kind.is_file() {
                found.push(entry.path());
            }
            // Never recurse into symlinks; the library must remain within the chosen root.
        }
    }
    found.sort();
    Ok(found)
}
fn midi(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok()
        && &magic == b"MThd"
}

async fn probe(path: &Path, settings: &Settings) -> Result<Value> {
    if midi(path) {
        return Ok(json!({"streams":[{"codec_type":"audio"}],"format":{}}));
    }
    let output = tokio::time::timeout(
        Duration::from_secs(settings.probe_timeout),
        tokio::process::Command::new(&settings.ffprobe)
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_type,duration:stream_tags=title,artist:format=duration:format_tags=title,artist",
                "-of",
                "json",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    if !output.status.success() || output.stdout.len() > 65536 {
        return Err("Unsupported or invalid audio".into());
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    if value["streams"].as_array().is_none_or(|s| s.is_empty()) {
        return Err("No audio stream".into());
    }
    Ok(value)
}
fn tag<'a>(info: &'a Value, name: &str) -> Option<&'a str> {
    [&info["format"]["tags"], &info["streams"][0]["tags"]]
        .into_iter()
        .filter_map(Value::as_object)
        .flat_map(|tags| tags.iter())
        .find_map(|(key, value)| {
            key.eq_ignore_ascii_case(name)
                .then(|| value.as_str())
                .flatten()
        })
}

async fn scan(settings: &Settings) -> Result<Index> {
    let root = root(settings)?;
    let check = tokio::process::Command::new(&settings.ffprobe)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    if !tokio::time::timeout(Duration::from_secs(5), check)
        .await??
        .success()
    {
        return Err("Установите FFmpeg/ffprobe или задайте ffprobe в настройках local".into());
    }
    let cached: Index = support::read_json(&index_path()).unwrap_or_default();
    let cached: HashMap<_, _> = if cached.root == root {
        cached
            .entries
            .into_iter()
            .map(|e| (e.path.clone(), e))
            .collect()
    } else {
        HashMap::new()
    };
    let mut index = Index {
        root: root.clone(),
        entries: Vec::new(),
    };
    let mut tasks = tokio::task::JoinSet::new();
    for path in files(&root, settings)? {
        let meta = std::fs::metadata(&path)?;
        let modified = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let relative = path.strip_prefix(&root)?.to_owned();
        if let Some(entry) = cached
            .get(&relative)
            .filter(|e| e.size == meta.len() && e.modified == modified)
        {
            index.entries.push(entry.clone());
            continue;
        }
        let settings = settings.clone();
        tasks.spawn(async move {
            let info = match probe(&path, &settings).await {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Пропуск {}: {error}", relative.display());
                    return None;
                }
            };
            let seconds = info["format"]["duration"]
                .as_str()
                .or_else(|| info["streams"][0]["duration"].as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let id = format!(
                "{:x}",
                Sha256::digest(relative.to_string_lossy().as_bytes())
            );
            Some(Entry {
                id,
                path: relative,
                size: meta.len(),
                modified,
                title: tag(&info, "title").map(str::to_owned).unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into()
                }),
                artist: tag(&info, "artist").unwrap_or("").into(),
                duration_ms: (seconds.max(0.0) * 1000.0) as i64,
            })
        });
        if tasks.len() >= 8 {
            if let Some(entry) = tasks.join_next().await.expect("pending probe")? {
                index.entries.push(entry);
            }
            if index.entries.len().is_multiple_of(64) {
                support::save(&index_path(), &index)?;
            }
        }
    }
    while let Some(result) = tasks.join_next().await {
        if let Some(entry) = result? {
            index.entries.push(entry);
        }
    }
    index.entries.sort_by(|a, b| a.path.cmp(&b.path));
    support::save(&index_path(), &index)?;
    Ok(index)
}
fn resolve(index: &Index, configured: &Path, id: &str) -> Result<PathBuf> {
    if configured != index.root {
        return Err("Library changed; refresh tracks".into());
    }
    let entry = index
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or("Unknown local track")?;
    let path = std::fs::canonicalize(index.root.join(&entry.path))?;
    if !path.starts_with(configured) || !path.is_file() {
        return Err("Audio path outside library".into());
    }
    Ok(path)
}

fn set(settings: &mut Settings, key: &str, value: &str) -> Result<()> {
    match key {
        "path" => {
            let value = support::text(value, 4096)?;
            let path = if let Some(rest) = value.strip_prefix("~/") {
                PathBuf::from(std::env::var("HOME")?).join(rest)
            } else {
                PathBuf::from(value)
            };
            settings.path = std::fs::canonicalize(path)?.to_string_lossy().into();
            root(settings)?;
        }
        "recursive" => settings.recursive = support::boolean(value)?,
        "hidden" => settings.hidden = support::boolean(value)?,
        "shuffle" => settings.shuffle = support::boolean(value)?,
        "scan_limit" => {
            let number = value.parse()?;
            if !(1..=100000).contains(&number) {
                return Err("scan_limit: 1..100000".into());
            }
            settings.scan_limit = number;
        }
        "probe_timeout" => {
            let number = value.parse()?;
            if !(1..=30).contains(&number) {
                return Err("probe_timeout: 1..30 seconds".into());
            }
            settings.probe_timeout = number;
        }
        "ffmpeg" => settings.ffmpeg = support::text(value, 4096)?,
        "ffprobe" => settings.ffprobe = support::text(value, 4096)?,
        "fluidsynth" => settings.fluidsynth = support::text(value, 4096)?,
        "soundfont" => {
            let path = std::fs::canonicalize(value)?;
            if !path.is_file() {
                return Err("soundfont must be an SF2/SF3 file".into());
            }
            settings.soundfont = path.to_string_lossy().into();
        }
        _ => return Err(format!("Unknown setting: {key}").into()),
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    let arg = |n| args.get(n).map(String::as_str).ok_or("Missing argument");
    if command == "info" {
        return support::print_json(
            json!({"protocol":1,"id":"local","name":"Локальная папка","default_playlist":"all"}),
        );
    }
    let mut settings: Settings = support::load("mz-local")?;
    match command {
        "settings" => support::print_json(json!({"settings": settings})),
        "set-setting" => {
            set(&mut settings, arg(1)?, arg(2)?)?;
            support::save_settings("mz-local", &settings)?;
            support::print_json(json!({"settings": settings}))
        }
        "playlists" => support::print_json(
            json!({"playlists":[{"id":"all","name":if settings.path.is_empty() { "Локальная папка" } else { &settings.path }}]}),
        ),
        "tracks" => {
            if arg(1)? != "all" {
                return Err("Unknown local playlist".into());
            }
            let mut index = scan(&settings).await?;
            if settings.shuffle {
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)?
                    .as_nanos()
                    .to_string();
                index
                    .entries
                    .sort_by_key(|e| Sha256::digest(format!("{seed}{}", e.id)));
            }
            support::print_json(
                json!({"tracks":index.entries.iter().map(|e| json!({"id":e.id,"title":e.title,"artist":e.artist,"duration_ms":e.duration_ms})).collect::<Vec<_>>()}),
            )
        }
        "audio" => {
            let index: Index = support::read_json(&index_path())?;
            let configured = root(&settings)?;
            let path = resolve(&index, &configured, arg(1)?)?;
            let mut rendered = None;
            if midi(&path) {
                if settings.soundfont.is_empty() {
                    return Err("MIDI: установите fluidsynth и задайте mz set local soundfont /путь/банк.sf2".into());
                }
                let file = tempfile::Builder::new().suffix(".wav").tempfile()?;
                let status = tokio::time::timeout(
                    Duration::from_secs(90),
                    tokio::process::Command::new(&settings.fluidsynth)
                        .args(["-ni", "-T", "wav", "-F"])
                        .arg(file.path())
                        .args(["-r", "48000"])
                        .arg(&settings.soundfont)
                        .arg(&path)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::inherit())
                        .kill_on_drop(true)
                        .status(),
                )
                .await??;
                if !status.success() {
                    return Err(format!("FluidSynth exited: {status}").into());
                }
                rendered = Some(file.into_file());
            }
            let mut ffmpeg = Command::new(&settings.ffmpeg);
            ffmpeg
                .args([
                    "-nostdin",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-protocol_whitelist",
                    "file,pipe",
                    "-i",
                ])
                .arg(if rendered.is_some() {
                    std::ffi::OsStr::new("pipe:0")
                } else {
                    path.as_os_str()
                })
                .args([
                    "-map", "0:a:0", "-vn", "-c:a", "flac", "-f", "flac", "pipe:1",
                ]);
            support::run_ffmpeg_input(ffmpeg, rendered)
        }
        "login" => {
            eprintln!(
                "Для локальной музыки вход не нужен. Укажите path через mz set local path ..."
            );
            Ok(())
        }
        _ => Err("Unknown local command".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ogg_stream_tags_and_case_are_supported() {
        let info = json!({"streams":[{"tags":{"TITLE":"Old song", "ARTIST":"Artist"}}]});
        assert_eq!(tag(&info, "title"), Some("Old song"));
        assert_eq!(tag(&info, "artist"), Some("Artist"));
    }
    #[test]
    fn traversal_cannot_escape_the_music_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("music")).unwrap();
        std::fs::write(dir.path().join("outside.flac"), b"private").unwrap();
        let root = std::fs::canonicalize(dir.path().join("music")).unwrap();
        let index = Index {
            root: root.clone(),
            entries: vec![Entry {
                id: "bad".into(),
                path: "../outside.flac".into(),
                size: 0,
                modified: 0,
                title: String::new(),
                artist: String::new(),
                duration_ms: 0,
            }],
        };
        assert!(resolve(&index, &root, "bad").is_err());
    }
    #[test]
    fn scan_has_no_extension_whitelist_and_honors_recursion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("music.unknown"), b"audio").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/tone.flac"), b"audio").unwrap();
        let mut settings = Settings::default();
        assert_eq!(files(dir.path(), &settings).unwrap().len(), 2);
        settings.recursive = false;
        assert_eq!(files(dir.path(), &settings).unwrap().len(), 1);
        settings.scan_limit = 1;
        assert!(files(dir.path(), &settings).is_err());
    }
    #[test]
    fn midi_is_detected_by_header_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old-song");
        std::fs::write(&path, b"MThd\0\0\0\x06").unwrap();
        assert!(midi(&path));
    }
}
