use crate::paths::config_dir;
use std::env;
use std::fs;

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
