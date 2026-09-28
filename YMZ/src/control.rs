use crate::api::{WaveSettings, YandexClient};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use zbus::interface;

pub struct YmzControl {
    pub ym: Arc<YandexClient>,
    pub switch_tx: mpsc::UnboundedSender<String>,
    pub playlist_id: Arc<RwLock<String>>,
    pub wave: Arc<RwLock<WaveSettings>>,
    pub current_title: Arc<RwLock<String>>,
    pub current_artist: Arc<RwLock<String>>,
}

#[interface(name = "org.mcz.Control")]
impl YmzControl {
    async fn list_playlists(&self) -> Vec<(String, String)> {
        match self.ym.list_playlists().await {
            Ok(list) => list,
            Err(e) => {
                log::warn!("Не получить плейлисты: {e}");
                vec![("wave".into(), "Моя волна".into())]
            }
        }
    }

    async fn set_playlist(&self, id: &str) -> zbus::fdo::Result<()> {
        if id != "wave"
            && id
                .strip_prefix("playlist:")
                .and_then(|v| v.parse::<u64>().ok())
                .is_none()
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Некорректный ID плейлиста".into(),
            ));
        }
        if let Err(e) = std::fs::create_dir_all(mcz::paths::config_dir("ymz")) {
            log::warn!("Не создать каталог настроек: {e}");
        } else if let Err(e) = std::fs::write(mcz::paths::config_dir("ymz").join("playlist"), id) {
            log::warn!("Не сохранить выбор плейлиста: {e}");
        }
        self.switch_tx
            .send(id.to_owned())
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn current_playlist(&self) -> String {
        self.playlist_id.read().await.clone()
    }
    async fn now_playing(&self) -> (String, String) {
        (
            self.current_artist.read().await.clone(),
            self.current_title.read().await.clone(),
        )
    }

    async fn wave_settings(&self) -> (String, String, String) {
        let w = self.wave.read().await;
        (w.mood.clone(), w.diversity.clone(), w.language.clone())
    }

    async fn set_wave_setting(&self, key: &str, value: &str) -> zbus::fdo::Result<()> {
        let mut settings = self.wave.read().await.clone();
        match key {
            "mood" => settings.mood = value.into(),
            "diversity" => settings.diversity = value.into(),
            "language" => settings.language = value.into(),
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "Неизвестная настройка".into(),
                ))
            }
        }
        self.ym
            .set_wave_settings(&settings)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        *self.wave.write().await = settings;
        if self.playlist_id.read().await.as_str() == "wave" {
            let _ = self.switch_tx.send("wave".into());
        }
        Ok(())
    }
}
