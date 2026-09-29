use crate::paths::config_dir;
use std::env;
use std::fs;

const APP_DIR: &str = "youmz";

#[derive(Clone)]
pub struct Config {
    /// Cookie-строка браузера (SAPISID / __Secure-3PAPISID и т.д.)
    pub cookie: Option<String>,
    /// ID плейлиста/микса: "RDMM" — личный микс ("Мой джем"),
    /// "RDAMVM<videoId>" — радио от трека, либо ID любого плейлиста
    pub playlist_id: String,
    /// Базовый URL InnerTube. По умолчанию https://www.youtube.com/youtubei/v1.
    /// Переопределяется через YOUMZ_BASE_URL (удобно для тестов/корпоративных прокси).
    #[allow(dead_code)]
    pub base_url: String,
    /// Необязательный прокси для всех запросов, например socks5h://localhost:2080
    pub proxy: Option<String>,
    pub prefetch_concurrency: usize,
    pub prefetch_count: usize,
    pub request_timeout_secs: u64,
    pub retry_attempts: u32,
}

fn read_file_or_env(file_name: &str, env_var: &str) -> Option<String> {
    {
        let path = config_dir(APP_DIR).join(file_name);
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    env::var(env_var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn load() -> Result<Config, String> {
    // Cookie больше не обязателен: при первом запуске программа предложит
    // войти по ссылке или импортирует сессию YouTube Music Desktop.
    let cookie = read_file_or_env("cookie", "YOUMZ_COOKIE");

    let playlist_id =
        read_file_or_env("playlist", "YOUMZ_PLAYLIST").unwrap_or_else(|| "RDMM".to_string());

    // music.youtube.com: с www.youtube.com InnerTube отдаёт анонимный контент
    let base_url = read_file_or_env("base_url", "YOUMZ_BASE_URL")
        .unwrap_or_else(|| "https://music.youtube.com/youtubei/v1".to_string());

    let proxy = read_file_or_env("proxy", "YOUMZ_PROXY");
    let usize_env = |name: &str, default: usize| {
        env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let u64_env = |name: &str, default: u64| {
        env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let u32_env = |name: &str, default: u32| {
        env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };

    Ok(Config {
        cookie,
        playlist_id,
        base_url,
        proxy,
        prefetch_concurrency: usize_env("YOUMZ_PREFETCH_CONCURRENCY", 2).max(1),
        prefetch_count: usize_env("YOUMZ_PREFETCH_COUNT", 2).max(1),
        request_timeout_secs: u64_env("YOUMZ_REQUEST_TIMEOUT_SECS", 20).max(5),
        retry_attempts: u32_env("YOUMZ_RETRY_ATTEMPTS", 3).clamp(1, 8),
    })
}
