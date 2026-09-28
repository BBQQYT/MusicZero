//! Получение треков и аудиопотоков через rustypipe.
//!
//! rustypipe берёт на себя всю работу с InnerTube: deobfuscation подписей,
//! PO-токены, visitorData и переключение клиентов. Нам остаётся только
//! выбрать подходящий аудиопоток и скачать его.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rustypipe::client::{RustyPipe, RustyPipeBuilder};
use rustypipe::model::TrackItem;

use crate::auth::Auth;
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
    cfg: Arc<Config>,
    /// HTTP-клиент (с настроенным прокси) для скачивания обложек и др. ресурсов
    http: reqwest::Client,
    /// Кэш предзагруженных аудиоданных: video_id -> байты
    cache: Arc<tokio::sync::Mutex<HashMap<String, Vec<u8>>>>,
    inflight: Arc<tokio::sync::Mutex<HashSet<String>>>,
    prefetch_semaphore: Arc<tokio::sync::Semaphore>,
}

impl YtClient {
    /// Создать клиент. Cookie включают авторизованный режим rustypipe —
    /// без него «Мой джем» недоступен.
    pub async fn new(cfg: Arc<Config>, _auth: Auth) -> Self {
        let make_builder = || {
            let mut b = reqwest::Client::builder();
            if let Some(proxy_url) = &cfg.proxy {
                b = b.proxy(reqwest::Proxy::all(proxy_url).expect("Некорректный адрес прокси"));
            }
            b
        };

        let http = make_builder()
            .build()
            .expect("Не удалось создать HTTP-клиент");

        let client_builder = make_builder();

        let builder = RustyPipeBuilder::new()
            // Кэш rustypipe (visitorData, cookie) — в конфиге youmz
            .storage_dir(crate::auth::config_dir());

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

        Self {
            rp: Arc::new(rp),
            cfg: cfg.clone(),
            http,
            cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            inflight: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            prefetch_semaphore: Arc::new(tokio::sync::Semaphore::new(cfg.prefetch_concurrency)),
        }
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

    /// Скачать аудио в память через yt-dlp (или взять из кэша предзагрузки).
    pub async fn fetch_audio(&self, video_id: &str) -> Result<Vec<u8>, String> {
        if let Some(bytes) = self.cache.lock().await.get(video_id).cloned() {
            return Ok(bytes);
        }

        let _permit = self
            .prefetch_semaphore
            .acquire()
            .await
            .map_err(|_| "prefetch semaphore закрыт".to_string())?;
        let cookie_path = crate::auth::ensure_netscape_cookie_file();
        let attempts = self.cfg.retry_attempts.max(1);
        let mut delay = Duration::from_millis(500);

        for attempt in 1..=attempts {
            let mut cmd = tokio::process::Command::new("yt-dlp");
            cmd.arg("-f")
                .arg("140/ba/bestaudio")
                .arg("--no-playlist")
                .arg("--no-warnings")
                .arg("--no-progress")
                .arg("-o")
                .arg("-")
                .arg(format!("https://www.youtube.com/watch?v={video_id}"));
            if let Some(ref cp) = cookie_path {
                cmd.arg("--cookies").arg(cp);
            }
            if let Some(ref proxy) = self.cfg.proxy {
                cmd.arg("--proxy").arg(proxy);
            }

            match tokio::time::timeout(
                Duration::from_secs(self.cfg.request_timeout_secs),
                cmd.output(),
            )
            .await
            {
                Ok(Ok(output)) if output.status.success() && !output.stdout.is_empty() => {
                    return Ok(output.stdout);
                }
                Ok(Ok(output)) => {
                    let err = String::from_utf8_lossy(&output.stderr);
                    log::warn!("yt-dlp ({video_id}) attempt {attempt}/{attempts}: {err}");
                }
                Ok(Err(e)) => log::warn!("yt-dlp ({video_id}) attempt {attempt}/{attempts}: {e}"),
                Err(_) => log::warn!(
                    "yt-dlp ({video_id}) timeout {}s, attempt {attempt}/{attempts}",
                    self.cfg.request_timeout_secs
                ),
            }
            if attempt < attempts {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(8));
            }
        }
        Err(format!(
            "yt-dlp ({video_id}): exhausted {} attempts",
            attempts
        ))
    }

    /// Предзагрузить следующий трек и его обложку в фон, чтобы скип был мгновенным
    pub fn try_preload(self: &Arc<Self>, track: Track) {
        let client = self.clone();
        tokio::spawn(async move {
            if client.cache.lock().await.contains_key(&track.video_id) {
                return;
            }
            {
                let mut inflight = client.inflight.lock().await;
                if !inflight.insert(track.video_id.clone()) {
                    return;
                }
            }

            let video_id = track.video_id.clone();
            let art_url = track.art_url.clone();
            let cover_client = client.clone();
            tokio::spawn(async move {
                let _ = cover_client
                    .fetch_and_cache_cover(&video_id, &art_url)
                    .await;
            });

            let result = client.fetch_audio(&track.video_id).await;
            client.inflight.lock().await.remove(&track.video_id);

            match result {
                Ok(bytes) => {
                    let mut cache = client.cache.lock().await;
                    while cache.len() >= client.cfg.prefetch_count {
                        if let Some(key) = cache.keys().next().cloned() {
                            cache.remove(&key);
                        } else {
                            break;
                        }
                    }
                    cache.insert(track.video_id.clone(), bytes);
                    log::info!("⚡ Предзагружен: {} — {}", track.artist, track.title);
                }
                Err(e) => log::warn!("Предзагрузка «{} — {}»: {e}", track.artist, track.title),
            }
        });
    }

    /// Скачать и сохранить обложку трека локально в ~/.cache/youmz/covers,
    /// вернув file:// URI. Dank Linux (шторка QuickShell) и системные виджеты
    /// требуют локальный файл, так как не могут сами загрузить URL через прокси.
    pub async fn fetch_and_cache_cover(&self, video_id: &str, art_url: &str) -> Option<String> {
        if art_url.is_empty() {
            return None;
        }

        let covers_dir = crate::auth::covers_dir();
        let _ = tokio::fs::create_dir_all(&covers_dir).await;
        let file_path = covers_dir.join(format!("{video_id}.jpg"));

        if file_path.exists() {
            return Some(format!("file://{}", file_path.display()));
        }

        match self
            .http
            .get(art_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(bytes) = resp.bytes().await {
                    if !bytes.is_empty() && tokio::fs::write(&file_path, &bytes).await.is_ok() {
                        log::debug!("Сохранена обложка для {video_id}: {}", file_path.display());
                        return Some(format!("file://{}", file_path.display()));
                    }
                }
            }
            Ok(resp) => {
                log::warn!(
                    "Не удалось скачать обложку для {video_id}: HTTP {}",
                    resp.status()
                );
            }
            Err(e) => {
                log::warn!("Ошибка загрузки обложки для {video_id}: {e}");
            }
        }

        Some(art_url.to_string())
    }

    /// Забрать предзагруженные байты, если они есть
    pub async fn take_cached(&self, video_id: &str) -> Option<Vec<u8>> {
        let mut cache = self.cache.lock().await;
        cache.remove(video_id)
    }

    /// Освободить кэш трека (после воспроизведения)
    pub async fn forget(&self, video_id: &str) {
        let mut cache = self.cache.lock().await;
        cache.remove(video_id);
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
