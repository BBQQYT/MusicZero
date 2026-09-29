use crate::paths::config_dir;
use std::env;
use std::fs;
use std::io::Write;

pub fn save_token(token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() || token.chars().any(char::is_control) {
        return Err("Токен пуст или содержит управляющие символы".into());
    }
    let dir = config_dir("ymz");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if path.exists() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(token.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(token.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn load_token() -> Result<String, String> {
    // 1. Проверяем ~/.config/ymz/token
    {
        let path = config_dir("ymz").join("token");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }
        }
    }

    // 2. Фолбэк на переменную окружения
    if let Ok(token) = env::var("YM_TOKEN") {
        let trimmed = token.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    Err(format!(
        "Токен не найден! Создайте {} или передайте YM_TOKEN",
        config_dir("ymz").join("token").display()
    ))
}

pub fn load_playlist() -> String {
    fs::read_to_string(config_dir("ymz").join("playlist"))
        .ok()
        .or_else(|| env::var("YMZ_PLAYLIST").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| {
            s == "wave"
                || s.strip_prefix("playlist:")
                    .and_then(|v| v.parse::<u64>().ok())
                    .is_some()
        })
        .unwrap_or_else(|| "wave".into())
}
