use crate::paths::config_dir;
use std::env;
use std::fs;
use std::io::Write;

const APP_DIR: &str = "youmz";

pub fn text<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if env::var("MZ_LANGUAGE").as_deref() == Ok("en") {
        en
    } else {
        ru
    }
}

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
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() <= 4096 {
                if let Ok(content) = fs::read_to_string(&path) {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() && trimmed.len() <= 4096 {
                        return Some(trimmed);
                    }
                }
            }
        }
    }

    env::var(env_var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= 4096)
}

pub fn load() -> Result<Config, String> {
    let cookie = crate::auth::load_session();

    let proxy = read_file_or_env("proxy", "YOUMZ_PROXY");

    Ok(Config { cookie, proxy })
}

pub fn browser() -> Option<String> {
    env::var("YOUMZ_BROWSER")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| read_file_or_env("browser", "YOUMZ_BROWSER"))
}

pub fn settings() -> Result<serde_json::Value, String> {
    let cfg = load()?;
    Ok(serde_json::json!({"settings": {
        "proxy": cfg.proxy.unwrap_or_default(),
        "browser": browser().unwrap_or_default(),
        "session": if cfg.cookie.is_some() { "***" } else { "" }
    }}))
}

pub fn set(key: &str, value: &str) -> mz_module_support::Result<()> {
    if key == "session" {
        return Ok(crate::auth::save_session(value)?);
    }
    if !matches!(key, "proxy" | "browser") {
        return Err("Unknown YouMZ setting".into());
    }
    mz_module_support::text(value, if key == "proxy" { 1024 } else { 4096 })?;
    if key == "proxy" && !value.is_empty() {
        if value.chars().any(char::is_whitespace) {
            return Err("Invalid proxy".into());
        }
        reqwest::Proxy::all(value)?;
    }
    let dir = config_dir(APP_DIR);
    fs::create_dir_all(&dir)?;
    let mut file = tempfile::NamedTempFile::new_in(&dir)?;
    file.write_all(value.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(dir.join(key))?;
    Ok(())
}
