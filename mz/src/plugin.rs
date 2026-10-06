use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub art_url: String,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_buffer_ms")]
    pub buffer_ms: u32,
    #[serde(default, skip_serializing)]
    pub feedback: String,
}

fn default_buffer_ms() -> u32 {
    1000
}

#[derive(Clone, Debug, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct TrackList {
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub continuous: bool,
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
    let settings = crate::config::Settings::load()?;
    if !settings.modules_dir.is_empty() {
        return Ok(PathBuf::from(settings.modules_dir));
    }
    Ok(std::env::current_exe()?
        .parent()
        .ok_or("Executable directory missing")?
        .join("modules"))
}

pub fn discover() -> Result<Vec<Module>> {
    let root = module_dir()?;
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut modules = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.path();
        let manifest_path = folder.join("module.json");
        if !manifest_path.is_file() {
            continue;
        }
        let text = match read_text_limited(&manifest_path, 64 * 1024) {
            Ok(t) if t.len() <= 64 * 1024 => t,
            Ok(_) => {
                eprintln!(
                    "Некорректный модуль (слишком большой): {}",
                    manifest_path.display()
                );
                continue;
            }
            Err(_) => {
                eprintln!("Некорректный модуль: {}", manifest_path.display());
                continue;
            }
        };
        let manifest: Manifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => {
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
        let executable = folder.join(&name);
        let executable = match std::fs::canonicalize(&executable) {
            Ok(p) if p.is_file() => p,
            _ => {
                eprintln!(
                    "Бинарник модуля не найден: {}",
                    folder.join(&name).display()
                );
                continue;
            }
        };
        if !executable.starts_with(&root) {
            eprintln!(
                "Бинарник модуля вне директории модулей: {}",
                executable.display()
            );
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&executable) {
                if meta.permissions().mode() & 0o022 != 0 {
                    eprintln!(
                        "Бинарник модуля доступен на запись группе/другим: {}",
                        executable.display()
                    );
                    continue;
                }
            }
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
    pub(crate) fn command(&self, action: &str) -> Result<Command> {
        let settings = crate::config::Settings::load()?;
        let mut command = Command::new(&self.executable);
        command
            .arg(action)
            .env("MZ_LANGUAGE", settings.language.code());
        if !settings.temp_dir.is_empty() {
            for key in ["TMPDIR", "TMP", "TEMP"] {
                command.env(key, &settings.temp_dir);
            }
        }
        Ok(command)
    }
    pub async fn json(&self, command: &str, args: &[&str]) -> Result<Value> {
        self.json_request(command, args, false).await
    }
    pub async fn config_json(&self, command: &str, args: &[&str]) -> Result<Value> {
        self.json_request(command, args, true).await
    }
    async fn json_request(&self, command: &str, args: &[&str], capture: bool) -> Result<Value> {
        let mut child = self
            .command(command)?
            .args(args)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(if capture {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .spawn()?;
        let stdout = child.stdout.take().ok_or("Module stdout unavailable")?;
        let stderr = child.stderr.take();
        let response = tokio::time::timeout(Duration::from_secs(90), async {
            let output = async {
                let mut bytes = Vec::new();
                stdout
                    .take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .await?;
                Ok::<_, std::io::Error>(bytes)
            };
            let errors = async {
                let mut bytes = Vec::new();
                if let Some(stderr) = stderr {
                    stderr.take(64 * 1024).read_to_end(&mut bytes).await?;
                }
                Ok::<_, std::io::Error>(bytes)
            };
            let (bytes, errors) = tokio::try_join!(output, errors)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err("Module JSON response exceeds 4 MiB".into());
            }
            let status = child.wait().await?;
            if !status.success() {
                return Err(format!(
                    "{}: {command}: {status}\n{}",
                    self.manifest.id,
                    String::from_utf8_lossy(&errors)
                )
                .into());
            }
            Ok::<_, Box<dyn Error + Send + Sync>>(bytes)
        })
        .await??;
        Ok(serde_json::from_slice(&response)?)
    }

    pub async fn validate(&self) -> Result<()> {
        let info = self.config_json("info", &[]).await?;
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

    pub async fn tracks(&self, playlist: &str, after: Option<&str>) -> Result<TrackList> {
        let mut args = vec![playlist];
        if let Some(after) = after {
            args.push(after);
        }
        Ok(serde_json::from_value(self.json("tracks", &args).await?)?)
    }

    pub async fn audio(&self, track_id: &str) -> Result<NamedTempFile> {
        let file = crate::config::temp_file()?;
        let mut child = self
            .command("audio")?
            .arg(track_id)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("Module stdout unavailable")?;
        let mut async_file = tokio::fs::File::from_std(file.reopen()?);
        tokio::time::timeout(Duration::from_secs(180), async {
            let copied =
                tokio::io::copy(&mut stdout.take(512 * 1024 * 1024 + 1), &mut async_file).await?;
            if copied == 0 || copied > 512 * 1024 * 1024 {
                return Err::<(), Box<dyn Error + Send + Sync>>(
                    "Module audio is empty or exceeds 512 MiB".into(),
                );
            }
            let status = child.wait().await?;
            if !status.success() {
                return Err(format!("Audio module exited: {status}").into());
            }
            async_file.flush().await?;
            Ok(())
        })
        .await??;
        Ok(file)
    }

    pub async fn login(&self) -> Result<()> {
        let status = self
            .command("login")?
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
    // id is validated by discover(); still sanitize for any caller.
    debug_assert!(valid_name(id), "playlist_file called with invalid id");
    let safe = if valid_name(id) { id } else { "default" };
    mcz::paths::config_dir("mz").join(format!("{safe}.playlist"))
}

pub fn selected_playlist(module: &Module) -> String {
    let raw = read_text_limited(&playlist_file(&module.manifest.id), 256)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| module.manifest.default_playlist.clone());
    if raw.is_empty() || raw.len() > 256 || raw.chars().any(char::is_control) {
        module.manifest.default_playlist.clone()
    } else {
        raw
    }
}

pub fn save_playlist(id: &str, playlist: &str) -> Result<()> {
    if !valid_name(id) {
        return Err(format!("Invalid module id: {id}").into());
    }
    if playlist.chars().any(char::is_control) {
        return Err("Invalid playlist id".into());
    }
    let sanitized = playlist.trim();
    if sanitized.is_empty() || sanitized.len() > 256 {
        return Err("Invalid playlist id".into());
    }
    let path = playlist_file(id);
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    // Atomic write + fsync to avoid partial/corrupt playlist files.
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    use std::io::Write;
    tmp.write_all(sanitized.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| e.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn read_text_limited(path: &Path, limit: u64) -> std::io::Result<String> {
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File too large",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_manifest_read_is_bounded() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&vec![b'x'; 65537]).unwrap();
        assert!(read_text_limited(file.path(), 65536).is_err());
    }
}
