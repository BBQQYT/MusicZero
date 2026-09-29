//! Получение метаданных YouTube Music через rustypipe.
//!
//! rustypipe берёт на себя всю работу с InnerTube: deobfuscation подписей,
//! PO-токены, visitorData и переключение клиентов. Нам остаётся только
//! Аудио модуль передаёт в хост отдельным потоком через yt-dlp.

use std::sync::Arc;

use rustypipe::client::{RustyPipe, RustyPipeBuilder};
use rustypipe::model::TrackItem;

use crate::config::Config;

/// YouTube просит войти в аккаунт ("вы не бот") — это ограничение IP/клиента,
/// а не свойство трека. Такой ответ надо отличать от обычной недоступности,
/// чтобы делать паузу вместо штурма запросами.
pub fn is_bot_check(reason: &str) -> bool {
    let r = reason.to_lowercase();
    r.contains("bot")
        || r.contains("не бот")
        || r.contains("sign in to confirm")
        || r.contains("войдите в аккаунт")
        || r.contains("try again")
        || r.contains("429")
        || r.contains("too many requests")
}

/// Плейлист из библиотеки пользователя (для меню в трее)
#[derive(Debug, Clone)]
pub struct PlaylistEntry {
    pub id: String,
    pub title: String,
}

/// Трек из панели микса
#[derive(Debug, Clone)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub art_url: String,
    pub duration_us: i64,
}

#[derive(Clone)]
pub struct YtClient {
    /// Клиент rustypipe: забирает миксы и метаданные библиотеки
    rp: Arc<RustyPipe>,
}

impl YtClient {
    /// Создать клиент. Cookie включают авторизованный режим rustypipe —
    /// без него «Мой джем» недоступен.
    pub async fn new(cfg: Arc<Config>) -> Self {
        let make_builder = || {
            let mut b = reqwest::Client::builder();
            if let Some(proxy_url) = &cfg.proxy {
                b = b.proxy(reqwest::Proxy::all(proxy_url).expect("Некорректный адрес прокси"));
            }
            b
        };

        let client_builder = make_builder();

        let builder = RustyPipeBuilder::new()
            // Кэш rustypipe (visitorData, cookie) — в конфиге youmz
            .storage_dir(crate::paths::config_dir("youmz"));

        let rp = builder
            .build_with_client(client_builder)
            .expect("Не удалось создать клиент rustypipe");

        // Авторизация cookie: rustypipe сам достанет SAPISIDHASH и применит
        if let Some(cookie) = &cfg.cookie {
            if let Err(e) = rp.user_auth_set_cookie(cookie).await {
                log::warn!("Не удалось применить cookie-сессию: {e}");
            } else {
                log::info!("Cookie-сессия применена к rustypipe");
            }
        }

        Self { rp: Arc::new(rp) }
    }

    /// Получить партию треков микса (радио). RDMM = «Мой джем».
    pub async fn get_mix_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, String> {
        if !playlist_id.starts_with("RD") {
            let mut playlist = self
                .rp
                .query()
                .authenticated()
                .music_playlist(playlist_id)
                .await
                .map_err(|e| format!("Ошибка получения плейлиста: {e}"))?;
            playlist
                .tracks
                .extend_limit(self.rp.query().authenticated(), 1000)
                .await
                .map_err(|e| format!("Ошибка продолжения плейлиста: {e}"))?;
            let tracks: Vec<Track> = playlist
                .tracks
                .items
                .iter()
                .map(track_item_to_track)
                .collect();
            if tracks.is_empty() {
                return Err("Плейлист пуст".into());
            }
            return Ok(tracks);
        }
        let radio_id = if playlist_id.starts_with("RD") {
            playlist_id.to_string()
        } else {
            format!("RDAMPL{playlist_id}")
        };

        let paginator = self
            .rp
            .query()
            .authenticated()
            .music_radio(&radio_id)
            .await
            .map_err(|e| format!("Ошибка получения микса: {e}"))?;

        let tracks = paginator
            .items
            .iter()
            .map(track_item_to_track)
            .collect::<Vec<_>>();

        if tracks.is_empty() {
            return Err("Микс вернул 0 треков".to_string());
        }
        Ok(tracks)
    }

    /// Список плейлистов пользователя для меню в трее.
    pub async fn list_playlists(&self) -> Result<Vec<PlaylistEntry>, String> {
        let mut page = self
            .rp
            .query()
            .authenticated()
            .music_saved_playlists()
            .await
            .map_err(|e| format!("Ошибка получения библиотеки: {e}"))?;
        page.extend_limit(self.rp.query().authenticated(), 100)
            .await
            .map_err(|e| format!("Ошибка продолжения библиотеки: {e}"))?;
        let mut entries = vec![PlaylistEntry {
            id: "LM".into(),
            title: "Понравившаяся музыка".into(),
        }];
        entries.extend(
            page.items
                .into_iter()
                .filter(|p| !p.is_podcast)
                .map(|p| PlaylistEntry {
                    id: p.id,
                    title: p.name,
                }),
        );
        Ok(entries)
    }
}

/// Преобразовать элемент трека rustypipe во внутреннее представление
fn track_item_to_track(item: &TrackItem) -> Track {
    let artist = item
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Неизвестный исполнитель".to_string());

    // Обложка: берём изображение максимального качества (max_by_key) для шторки и виджетов системы
    let art_url = item
        .cover
        .iter()
        .max_by_key(|t| t.width)
        .or_else(|| item.cover.first())
        .map(|t| t.url.clone())
        .unwrap_or_default();

    Track {
        video_id: item.id.clone(),
        title: item.name.clone(),
        artist,
        art_url,
        duration_us: item.duration.unwrap_or(0) as i64 * 1_000_000,
    }
}

/// История «уже сыгранного», чтобы радио не ходило по кругу
pub struct History {
    seen: std::collections::VecDeque<String>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.seen.iter().any(|s| s == id)
    }

    /// Очистить историю — например, при переключении плейлиста
    pub fn clear(&mut self) {
        self.seen.clear();
    }

    pub fn push(&mut self, id: String) {
        if self.seen.len() == self.capacity {
            self.seen.pop_front();
        }
        self.seen.push_back(id);
    }

    pub fn filter_fresh<'a>(&self, tracks: &'a [Track]) -> Vec<&'a Track> {
        let mut fresh = vec![];
        let mut blocked: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for t in tracks {
            if blocked.contains(t.video_id.as_str()) || self.contains(&t.video_id) {
                blocked.insert(&t.video_id);
                continue;
            }
            // Внутри одной партии могут быть дубликаты видео — оставляем первое вхождение
            if !fresh.iter().any(|f: &&Track| f.video_id == t.video_id) {
                fresh.push(t);
            }
        }
        fresh
    }
}
