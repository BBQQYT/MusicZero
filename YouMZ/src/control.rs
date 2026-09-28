//! Интерфейс управления демоном через D-Bus (org.mcz.Control).
//!
//! Используется треем youmz-tray: получает список плейлистов пользователя
//! и переключает текущий плейлист на лету, без перезапуска демона.

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use zbus::interface;

use crate::api::YtClient;

pub struct YoumzControl {
    pub yt: YtClient,
    /// Канал переключения плейлиста: Main loop обновит очередь
    pub switch_tx: mpsc::UnboundedSender<String>,
    /// Текущий плейлист
    pub playlist_id: Arc<RwLock<String>>,
    /// Что сейчас играет
    pub current_title: Arc<RwLock<String>>,
    pub current_artist: Arc<RwLock<String>>,
}

#[interface(name = "org.mcz.Control")]
impl YoumzControl {
    /// Список плейлистов пользователя: «Мой джем», «Понравившаяся музыка»
    /// и собственные плейлисты из библиотеки.
    pub async fn list_playlists(&self) -> Vec<(String, String)> {
        // «Мой джем» всегда первый — это персональный микс
        let mut list = vec![("RDMM".to_string(), "Мой джем".to_string())];

        match self.yt.list_playlists().await {
            Ok(user) => {
                for p in user {
                    if !list.iter().any(|(id, _)| id == &p.id) {
                        list.push((p.id, p.title));
                    }
                }
            }
            Err(e) => {
                log::warn!("Не удалось получить список плейлистов: {e}");
                // fallback: хотя бы «Понравившаяся музыка»
                list.push(("LM".to_string(), "Понравившаяся музыка".to_string()));
            }
        }

        list
    }

    /// Переключиться на другой плейлист. Очередь сбрасывается,
    /// текущий трек скипается.
    pub async fn set_playlist(&self, id: &str) {
        log::info!("Трей запросил плейлист: {id}");
        if let Err(e) = std::fs::create_dir_all(mcz::paths::config_dir("youmz")) {
            log::warn!("Не создать каталог настроек: {e}");
        } else if let Err(e) = std::fs::write(mcz::paths::config_dir("youmz").join("playlist"), id)
        {
            log::warn!("Не сохранить выбор плейлиста: {e}");
        }
        if let Err(e) = self.switch_tx.send(id.to_string()) {
            log::error!("Не удалось отправить команду переключения: {e}");
        }
    }

    /// Текущий плейлист
    pub async fn current_playlist(&self) -> String {
        self.playlist_id.read().await.clone()
    }

    /// Что сейчас играет: (исполнитель, название)
    pub async fn now_playing(&self) -> (String, String) {
        (
            self.current_artist.read().await.clone(),
            self.current_title.read().await.clone(),
        )
    }
}
