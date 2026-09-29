//! YouTube Music session obtained from a live browser, never from its profile files.

use crate::paths::config_dir;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const SESSION_FILE: &str = "session";

pub fn load_session() -> Option<String> {
    std::env::var("YOUMZ_SESSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fs::read_to_string(config_dir("youmz").join(SESSION_FILE)).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn save_session(session: &str) -> Result<(), String> {
    if session.is_empty() || session.chars().any(char::is_control) {
        return Err("Некорректная сессия YouTube Music".into());
    }
    let dir = config_dir("youmz");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(SESSION_FILE);
    #[cfg(unix)]
    if path.exists() {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).map_err(|error| error.to_string())?;
    file.write_all(session.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}
