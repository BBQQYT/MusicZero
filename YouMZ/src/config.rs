use crate::paths::config_dir;
use std::env;
use std::fs;

const APP_DIR: &str = "youmz";

#[derive(Clone)]
pub struct Config {
    /// Cookie-строка браузера (SAPISID / __Secure-3PAPISID и т.д.)
    pub cookie: Option<String>,
    /// Необязательный прокси для всех запросов, например socks5h://localhost:2080
    pub proxy: Option<String>,
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
    let cookie = crate::auth::load_session();

    let proxy = read_file_or_env("proxy", "YOUMZ_PROXY");

    Ok(Config { cookie, proxy })
}
