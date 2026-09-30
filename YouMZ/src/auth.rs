//! YouTube Music session obtained from a live browser, never from its profile files.

use crate::paths::config_dir;
use std::fs;
use std::io::Write;

const SESSION_FILE: &str = "session";

pub fn load_session() -> Option<String> {
    if let Ok(value) = std::env::var("YOUMZ_SESSION") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty()
            && trimmed.len() <= 32 * 1024
            && !trimmed.chars().any(char::is_control)
        {
            return Some(trimmed);
        }
        if !trimmed.is_empty() {
            log::warn!(
                "YOUMZ_SESSION слишком длинная ({}), игнорируется",
                trimmed.len()
            );
        }
    }
    let path = config_dir("youmz").join(SESSION_FILE);
    let meta = fs::metadata(&path).ok()?;
    if meta.len() > 32 * 1024 {
        log::warn!("Файл сессии слишком большой ({}), игнорируется", meta.len());
        return None;
    }
    fs::read_to_string(&path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| {
            !value.is_empty() && value.len() <= 32 * 1024 && !value.chars().any(char::is_control)
        })
}

pub fn save_session(session: &str) -> Result<(), String> {
    if session.is_empty()
        || session.len() > 32 * 1024
        || session
            .chars()
            .any(|c| c.is_control() || c == '\n' || c == '\r')
    {
        return Err("Некорректная сессия YouTube Music".into());
    }
    let dir = config_dir("youmz");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(SESSION_FILE);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600));
        tmp.write_all(session.as_bytes())
            .map_err(|error| error.to_string())?;
        tmp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        tmp.persist(&path).map_err(|e| e.error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
        tmp.write_all(session.as_bytes())
            .map_err(|error| error.to_string())?;
        tmp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        tmp.persist(&path).map_err(|e| e.error.to_string())?;
    }
    Ok(())
}
