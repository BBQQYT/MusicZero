use std::path::PathBuf;

/// Config directory for Linux desktops and Windows user profiles.
pub fn config_dir(app: &str) -> PathBuf {
    if cfg!(windows) {
        if let Some(base) = std::env::var_os("APPDATA") {
            return PathBuf::from(base).join(app);
        }
        if let Some(base) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(base)
                .join("AppData")
                .join("Roaming")
                .join(app);
        }
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        if !base.is_empty() {
            return PathBuf::from(base).join(app);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join(app);
    }
    PathBuf::from(".").join(app)
}

pub fn cache_dir(app: &str) -> PathBuf {
    if cfg!(windows) {
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(base).join(app);
        }
        if let Some(base) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(base).join("AppData").join("Local").join(app);
        }
    }
    if let Some(base) = std::env::var_os("XDG_CACHE_HOME") {
        if !base.is_empty() {
            return PathBuf::from(base).join(app);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache").join(app);
    }
    PathBuf::from(".").join(app).join("cache")
}
