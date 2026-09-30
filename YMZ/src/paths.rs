use std::path::PathBuf;

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
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(base).join(app);
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config").join(app))
        .unwrap_or_else(|| PathBuf::from(".").join(app))
}
