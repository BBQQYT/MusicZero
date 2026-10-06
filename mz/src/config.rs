use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Ru,
    En,
}
impl Language {
    pub fn text<'a>(self, ru: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Ru => ru,
            Self::En => en,
        }
    }
    pub fn code(self) -> &'static str {
        self.text("ru", "en")
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub modules_dir: String,
    pub log_filter: String,
    pub temp_dir: String,
    pub tray_enabled: bool,
    pub notifications_enabled: bool,
    pub service_module: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            modules_dir: String::new(),
            log_filter: String::new(),
            temp_dir: String::new(),
            tray_enabled: true,
            notifications_enabled: true,
            service_module: "ymz".into(),
        }
    }
}
impl Settings {
    pub fn load() -> Result<Self> {
        mz_module_support::load("mz")
    }
    pub fn save(&self) -> Result<()> {
        mz_module_support::save_settings("mz", self)
    }
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let text = mz_module_support::text(value, 4096)?;
        match key {
            "language" => {
                self.language = match value {
                    "ru" => Language::Ru,
                    "en" => Language::En,
                    _ => return Err("Expected ru or en".into()),
                }
            }
            "modules_dir" | "temp_dir" => {
                let text = if text.is_empty() {
                    text
                } else {
                    let path = if let Some(rest) = text.strip_prefix("~/") {
                        PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(rest)
                    } else {
                        PathBuf::from(&text)
                    };
                    let path = std::fs::canonicalize(path)?;
                    if !path.is_dir() {
                        return Err("Expected an existing directory".into());
                    }
                    path.to_str()
                        .ok_or("Directory must be valid UTF-8")?
                        .to_owned()
                };
                if key == "modules_dir" {
                    self.modules_dir = text;
                } else {
                    self.temp_dir = text;
                }
            }
            "log_filter" => self.log_filter = text,
            "tray_enabled" => self.tray_enabled = mz_module_support::boolean(value)?,
            "notifications_enabled" => {
                self.notifications_enabled = mz_module_support::boolean(value)?
            }
            "service_module" => {
                if text.is_empty()
                    || text.len() > 64
                    || !text
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
                {
                    return Err("Invalid service module ID".into());
                }
                self.service_module = text;
            }
            _ => return Err("Unknown host setting".into()),
        }
        Ok(())
    }
}

pub fn temp_file() -> Result<tempfile::NamedTempFile> {
    let settings = Settings::load()?;
    if settings.temp_dir.is_empty() {
        Ok(tempfile::NamedTempFile::new()?)
    } else {
        Ok(tempfile::NamedTempFile::new_in(settings.temp_dir)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_preferences_keep_the_tray_enabled_and_new_choices_round_trip() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"language":"en","log_filter":"warn"}"#).unwrap();
        assert!(settings.tray_enabled);
        assert!(settings.notifications_enabled);
        settings.set("notifications_enabled", "false").unwrap();
        settings.set("tray_enabled", "false").unwrap();
        settings.set("service_module", "youmz").unwrap();
        let loaded: Settings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert!(!loaded.tray_enabled);
        assert!(!loaded.notifications_enabled);
        assert_eq!(loaded.service_module, "youmz");
        assert_eq!(loaded.language, Language::En);
        assert!(settings.set("service_module", "../../evil").is_err());
        assert!(settings.set("tray_enabled", "maybe").is_err());
    }
}
