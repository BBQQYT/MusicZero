use crate::paths::config_dir;
use std::env;
use std::fs;
use std::io::Write;

pub fn save_token(token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty()
        || token.len() > 4096
        || token
            .chars()
            .any(|c| c.is_control() || c == '\n' || c == '\r')
    {
        return Err("Токен пуст или содержит управляющие символы".into());
    }
    let dir = config_dir("ymz");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Atomic write via temp file to avoid partial/corrupt token on crash.
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600));
        tmp.write_all(token.as_bytes())
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
        let dir2 = path.parent().unwrap_or(&dir);
        let mut tmp = tempfile::NamedTempFile::new_in(dir2).map_err(|e| e.to_string())?;
        tmp.write_all(token.as_bytes())
            .map_err(|error| error.to_string())?;
        tmp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        tmp.persist(&path).map_err(|e| e.error.to_string())?;
    }
    Ok(())
}

pub fn load_token() -> Result<String, String> {
    // 1. Проверяем ~/.config/ymz/token — с лимитом размера, чтобы не грузить гигабайты.
    {
        let path = config_dir("ymz").join("token");
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > 8192 {
                // Corrupt/oversized — ignore and fall through to env.
                log::warn!(
                    "Токен-файл слишком большой ({} байт), игнорируется",
                    meta.len()
                );
            } else if let Ok(content) = fs::read_to_string(&path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() && trimmed.len() <= 4096 {
                    return Ok(trimmed);
                }
            }
        }
    }

    // 2. Фолбэк на переменную окружения
    if let Ok(token) = env::var("YM_TOKEN") {
        let trimmed = token.trim().to_string();
        if !trimmed.is_empty() && trimmed.len() <= 4096 {
            return Ok(trimmed);
        }
    }

    Err(format!(
        "Токен не найден! Создайте {} или передайте YM_TOKEN",
        config_dir("ymz").join("token").display()
    ))
}

#[allow(dead_code)]
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
