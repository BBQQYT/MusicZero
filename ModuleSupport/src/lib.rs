use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[path = "../../MCZ/src/paths.rs"]
pub mod paths;

pub fn load<T: DeserializeOwned + Default>(app: &str) -> Result<T> {
    let path = paths::config_dir(app).join("settings.json");
    if !path.exists() {
        return Ok(T::default());
    }
    read_json(&path)
}
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 4 * 1024 * 1024 {
        return Err("Config/index exceeds 4 MiB".into());
    }
    Ok(serde_json::from_slice(&data)?)
}
pub fn save<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let dir = path.parent().ok_or("Invalid config path")?;
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    let bytes = serde_json::to_vec_pretty(data)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Config/index exceeds 4 MiB".into());
    }
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}
pub fn save_settings<T: Serialize>(app: &str, data: &T) -> Result<()> {
    save(&paths::config_dir(app).join("settings.json"), data)
}
pub fn print_json(value: serde_json::Value) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    Ok(())
}
pub fn run_ffmpeg(command: Command) -> Result<()> {
    run_ffmpeg_input(command, None)
}

pub fn run_ffmpeg_input(mut command: Command, input: Option<std::fs::File>) -> Result<()> {
    command
        .stdin(input.map(Stdio::from).unwrap_or_else(Stdio::null))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    // Replace the provider process so the host's cancellation kills FFmpeg too.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("FFmpeg exited: {status}").into())
        }
    }
}
pub fn boolean(value: &str) -> Result<bool> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err("Expected true or false".into()),
    }
}
pub fn text(value: &str, max: usize) -> Result<String> {
    if value.len() > max || value.chars().any(char::is_control) {
        return Err("Invalid setting value".into());
    }
    Ok(value.to_owned())
}
