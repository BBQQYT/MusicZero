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

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub modules_dir: String,
    pub log_filter: String,
    pub temp_dir: String,
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
