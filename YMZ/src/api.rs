use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

const BASE_URL: &str = "https://api.music.yandex.net";

#[derive(Deserialize, Debug)]
struct StationTracksResponse {
    result: StationResult,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StationResult {
    pub sequence: Vec<TrackEntry>,
    #[serde(rename = "batchId")]
    pub batch_id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TrackEntry {
    pub track: Option<Track>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    #[serde(rename = "coverUri")]
    pub cover_uri: Option<String>,
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<i64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Artist {
    pub name: String,
}

#[derive(Deserialize)]
struct AccountResponse {
    result: AccountResult,
}
#[derive(Deserialize)]
struct AccountResult {
    account: Account,
}
#[derive(Deserialize)]
struct Account {
    uid: u64,
}
#[derive(Deserialize)]
struct PlaylistsResponse {
    result: Vec<PlaylistInfo>,
}
#[derive(Deserialize)]
struct PlaylistInfo {
    kind: u64,
    title: String,
}
#[derive(Deserialize)]
struct PlaylistResponse {
    result: Playlist,
}
#[derive(Deserialize)]
struct Playlist {
    tracks: Vec<TrackEntry>,
}

#[derive(Clone, Debug)]
pub struct WaveSettings {
    pub mood: String,
    pub diversity: String,
    pub language: String,
}

impl Default for WaveSettings {
    fn default() -> Self {
        Self {
            mood: "all".into(),
            diversity: "default".into(),
            language: "any".into(),
        }
    }
}

#[derive(Deserialize, Debug)]
struct DownloadInfoResponse {
    result: Vec<DownloadInfo>,
}

#[derive(Deserialize, Debug)]
struct DownloadInfo {
    codec: String,
    #[serde(rename = "downloadInfoUrl")]
    download_info_url: String,
}

#[derive(Clone)]
pub struct YandexClient {
    client: reqwest::Client,
}

impl YandexClient {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token)).expect("Некорректный токен"),
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YandexMusicAndroid/24022371"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        Self { client }
    }

    async fn uid(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let response = self
            .client
            .get(format!("{BASE_URL}/account/status"))
            .send()
            .await?
            .error_for_status()?
            .json::<AccountResponse>()
            .await?;
        Ok(response.result.account.uid)
    }

    pub async fn list_playlists(
        &self,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error + Send + Sync>> {
        let uid = self.uid().await?;
        let response = self
            .client
            .get(format!("{BASE_URL}/users/{uid}/playlists/list"))
            .send()
            .await?
            .error_for_status()?
            .json::<PlaylistsResponse>()
            .await?;
        let mut result = vec![("wave".into(), "Моя волна".into())];
        result.extend(
            response
                .result
                .into_iter()
                .map(|p| (format!("playlist:{}", p.kind), p.title)),
        );
        Ok(result)
    }

    pub async fn get_playlist_tracks(
        &self,
        kind: u64,
    ) -> Result<Vec<TrackEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let uid = self.uid().await?;
        let response = self
            .client
            .get(format!("{BASE_URL}/users/{uid}/playlists/{kind}"))
            .send()
            .await?
            .error_for_status()?
            .json::<PlaylistResponse>()
            .await?;
        Ok(response.result.tracks)
    }

    pub async fn set_wave_settings(
        &self,
        settings: &WaveSettings,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !["all", "fun", "active", "calm", "sad"].contains(&settings.mood.as_str())
            || !["default", "favorite", "popular", "discover"]
                .contains(&settings.diversity.as_str())
            || !["any", "russian", "not-russian"].contains(&settings.language.as_str())
        {
            return Err("Некорректные настройки волны".into());
        }
        let response = self
            .client
            .post(format!(
                "{BASE_URL}/rotor/station/user:onyourwave/settings3"
            ))
            .form(&[
                ("moodEnergy", settings.mood.as_str()),
                ("diversity", settings.diversity.as_str()),
                ("language", settings.language.as_str()),
                ("type", "rotor"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let value: serde_json::Value = response.json().await?;
        if value.get("result").and_then(|r| r.as_str()) != Some("ok") {
            return Err(format!("API не принял настройки волны: {value}").into());
        }
        Ok(())
    }

    pub async fn get_wave_settings(
        &self,
    ) -> Result<WaveSettings, Box<dyn std::error::Error + Send + Sync>> {
        let response: serde_json::Value = self
            .client
            .get(format!("{BASE_URL}/rotor/station/user:onyourwave/info"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let settings = response
            .get("result")
            .and_then(|v| v.as_array())
            .and_then(|v| v.first())
            .and_then(|v| v.get("settings2"))
            .ok_or("В ответе станции нет settings2")?;
        let field = |key: &str| {
            settings
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        };
        Ok(WaveSettings {
            mood: field("moodEnergy").unwrap_or_else(|| "all".into()),
            diversity: field("diversity").unwrap_or_else(|| "default".into()),
            language: field("language").unwrap_or_else(|| "any".into()),
        })
    }

    // Запрос треков станции с автоматическим retry
    pub async fn get_wave_tracks(
        &self,
    ) -> Result<StationResult, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/rotor/station/user:onyourwave/tracks", BASE_URL);
        let mut retries = 3;
        let mut delay = Duration::from_secs(2);

        loop {
            match self
                .client
                .get(&url)
                .query(&[("settings2", "true")])
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    let data = resp.json::<StationTracksResponse>().await?;
                    return Ok(data.result);
                }
                Ok(resp) => {
                    log::warn!("API ответил со статусом {}. Повтор...", resp.status());
                }
                Err(e) => {
                    log::warn!("Сетевая ошибка при запросе волны: {}. Повтор...", e);
                }
            }

            retries -= 1;
            if retries == 0 {
                return Err("Превышено количество попыток подключения к API".into());
            }

            sleep(delay).await;
            delay *= 2;
        }
    }

    pub async fn send_feedback(&self, batch_id: &str, track_id: &str, event_type: &str) {
        let url = format!("{}/rotor/station/user:onyourwave/feedback", BASE_URL);
        let payload = [
            ("type", event_type.to_owned()),
            ("trackId", track_id.to_owned()),
            ("timestamp", chrono::Utc::now().timestamp().to_string()),
        ];

        // Feedback шлется fire-and-forget, не блокируя поток
        let client = self.client.clone();
        let batch_id = batch_id.to_owned();
        tokio::spawn(async move {
            let _ = client
                .post(&url)
                .query(&[("batch-id", batch_id)])
                .form(&payload)
                .send()
                .await;
        });
    }

    pub async fn get_stream_url(
        &self,
        track_id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let info_url = format!("{}/tracks/{}/download-info", BASE_URL, track_id);
        let mut delay = Duration::from_millis(500);
        let mut last = "unknown stream error".to_string();

        for attempt in 1..=4 {
            match self.get_stream_url_once(&info_url).await {
                Ok(url) => return Ok(url),
                Err(e) => {
                    last = e.to_string();
                    log::warn!("stream URL {track_id} attempt {attempt}/4: {last}");
                    if attempt < 4 {
                        sleep(delay).await;
                        delay = (delay * 2).min(Duration::from_secs(5));
                    }
                }
            }
        }
        Err(last.into())
    }

    async fn get_stream_url_once(
        &self,
        info_url: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.client.get(info_url).send().await?;
        if !resp.status().is_success() {
            return Err(format!("download-info HTTP {}", resp.status()).into());
        }
        let info_res = resp.json::<DownloadInfoResponse>().await?;
        let target = info_res
            .result
            .into_iter()
            .find(|d| d.codec == "mp3")
            .ok_or("MP3 поток не найден")?;

        let resp = self.client.get(&target.download_info_url).send().await?;
        if !resp.status().is_success() {
            return Err(format!("download-info XML HTTP {}", resp.status()).into());
        }
        let xml_raw = resp.text().await?;
        let doc = roxmltree::Document::parse(&xml_raw)?;

        let field = |name: &str| -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
            doc.descendants()
                .find(|n| n.has_tag_name(name))
                .and_then(|n| n.text())
                .ok_or_else(|| format!("В XML отсутствует поле {name}").into())
        };

        let host = field("host")?;
        let path = field("path")?;
        let ts = field("ts")?;
        let sign = field("s")?;
        let path_without_slash = path.strip_prefix('/').unwrap_or(path);

        let salt = "XGRlBW9FXlekgbPrr";
        let mut hasher = Md5::new();
        hasher.update(format!("{salt}{path_without_slash}{sign}").as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        Ok(format!("https://{host}/get-mp3/{hash}/{ts}{path}"))
    }

    pub async fn download_audio(
        &self,
        stream_url: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let mut delay = Duration::from_millis(500);
        for attempt in 1..=4 {
            match self.client.get(stream_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let bytes = resp.bytes().await?.to_vec();
                    if bytes.is_empty() {
                        return Err("Пустой аудиопоток".into());
                    }
                    return Ok(bytes);
                }
                Ok(resp) => log::warn!("audio HTTP {} attempt {attempt}/4", resp.status()),
                Err(e) => log::warn!("audio network {e} attempt {attempt}/4"),
            }
            if attempt < 4 {
                sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
        }
        Err("Не удалось получить аудиопоток после 4 попыток".into())
    }
}
