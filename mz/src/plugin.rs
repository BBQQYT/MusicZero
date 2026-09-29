use serde::Deserialize;
use serde_json::Value;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub protocol: u32,
    pub id: String,
    pub name: String,
    pub binary: String,
    pub default_playlist: String,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub manifest: Manifest,
    pub executable: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub art_url: String,
    #[serde(default)]
    pub duration_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct TrackList {
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct PlaylistList {
    playlists: Vec<Playlist>,
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

pub fn module_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MZ_MODULES_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(std::env::current_exe()?
        .parent()
        .ok_or("Executable directory missing")?
        .join("modules"))
}

pub fn discover() -> Result<Vec<Module>> {
    let root = module_dir()?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut modules = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.path();
        let manifest_path = folder.join("module.json");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest: Manifest = match std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
        {
            Some(manifest) => manifest,
            None => {
                eprintln!("Некорректный модуль: {}", manifest_path.display());
                continue;
            }
        };
        if manifest.protocol != 1
            || !valid_name(&manifest.id)
            || !valid_name(&manifest.binary)
            || entry.file_name() != manifest.id.as_str()
        {
            eprintln!("Несовместимый модуль: {}", manifest_path.display());
            continue;
        }
        let name = if cfg!(windows) {
            format!("{}.exe", manifest.binary)
        } else {
            manifest.binary.clone()
        };
        let executable = folder.join(name);
        if !executable.is_file() {
            eprintln!("Бинарник модуля не найден: {}", executable.display());
            continue;
        }
        modules.push(Module {
            manifest,
            executable,
        });
    }
    modules.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    Ok(modules)
}

impl Module {
    pub async fn json(&self, command: &str, args: &[&str]) -> Result<Value> {
        let mut child = Command::new(&self.executable)
            .arg(command)
            .args(args)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("Module stdout unavailable")?;
        let response = tokio::time::timeout(Duration::from_secs(90), async {
            let mut bytes = Vec::new();
            stdout
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err("Module JSON response exceeds 4 MiB".into());
            }
            let status = child.wait().await?;
            if !status.success() {
                return Err(
                    format!("{}: command {command} exited: {status}", self.manifest.id).into(),
                );
            }
            Ok::<_, Box<dyn Error + Send + Sync>>(bytes)
        })
        .await??;
        Ok(serde_json::from_slice(&response)?)
    }

    pub async fn validate(&self) -> Result<()> {
        let info = self.json("info", &[]).await?;
        if info["protocol"] != self.manifest.protocol
            || info["id"] != self.manifest.id
            || info["name"] != self.manifest.name
            || info["default_playlist"] != self.manifest.default_playlist
        {
            return Err(format!(
                "Module {} info does not match module.json",
                self.manifest.id
            )
            .into());
        }
        Ok(())
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        Ok(serde_json::from_value::<PlaylistList>(self.json("playlists", &[]).await?)?.playlists)
    }

    pub async fn tracks(&self, playlist: &str) -> Result<Vec<Track>> {
        Ok(serde_json::from_value::<TrackList>(self.json("tracks", &[playlist]).await?)?.tracks)
    }

    pub async fn audio(&self, track_id: &str) -> Result<NamedTempFile> {
        let file = NamedTempFile::new()?;
        let mut child = Command::new(&self.executable)
            .arg("audio")
            .arg(track_id)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("Module stdout unavailable")?;
        let mut async_file = tokio::fs::File::from_std(file.reopen()?);
        let copied = tokio::time::timeout(Duration::from_secs(180), async {
            tokio::io::copy(&mut stdout.take(512 * 1024 * 1024 + 1), &mut async_file).await
        })
        .await??;
        if copied == 0 || copied > 512 * 1024 * 1024 {
            child.kill().await?;
            return Err("Module audio is empty or exceeds 512 MiB".into());
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(format!("Audio module exited: {status}").into());
        }
        async_file.sync_all().await?;
        Ok(file)
    }

    pub async fn login(&self) -> Result<()> {
        let status = Command::new(&self.executable)
            .arg("login")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Module login exited: {status}").into())
        }
    }
}

pub fn playlist_file(id: &str) -> PathBuf {
    mcz::paths::config_dir("mz").join(format!("{id}.playlist"))
}

pub fn selected_playlist(module: &Module) -> String {
    std::fs::read_to_string(playlist_file(&module.manifest.id))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| module.manifest.default_playlist.clone())
}

pub fn save_playlist(id: &str, playlist: &str) -> Result<()> {
    let path = playlist_file(id);
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(path, playlist)?;
    Ok(())
}
