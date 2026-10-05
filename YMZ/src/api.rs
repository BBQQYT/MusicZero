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
    #[serde(rename = "batchId", default)]
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
    download_client: reqwest::Client,
}

impl YandexClient {
    pub fn new(token: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token))?,
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YandexMusicAndroid/24022371"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()?;
        let download_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(150))
            .build()?;

        Ok(Self {
            client,
            download_client,
        })
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
        after: Option<&str>,
    ) -> Result<StationResult, Box<dyn std::error::Error + Send + Sync>> {
        self.station_tracks(
            &format!("{BASE_URL}/rotor/station/user:onyourwave/tracks"),
            after,
        )
        .await
    }

    async fn station_tracks(
        &self,
        url: &str,
        after: Option<&str>,
    ) -> Result<StationResult, Box<dyn std::error::Error + Send + Sync>> {
        if after.is_some_and(|id| !Self::valid_track_id(id)) {
            return Err("Invalid wave continuation track id".into());
        }
        let mut retries = 3;
        let mut delay = Duration::from_secs(2);

        loop {
            let mut request = self.client.get(url).query(&[("settings2", "true")]);
            if let Some(after) = after {
                request = request.query(&[("queue", after)]);
            }
            match request.send().await {
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

    pub async fn send_feedback(
        &self,
        batch_id: &str,
        track_id: &str,
        event_type: &str,
        played_seconds: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.station_feedback(
            &format!("{BASE_URL}/rotor/station/user:onyourwave/feedback"),
            batch_id,
            track_id,
            event_type,
            played_seconds,
        )
        .await
    }

    async fn station_feedback(
        &self,
        url: &str,
        batch_id: &str,
        track_id: &str,
        event_type: &str,
        played_seconds: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !["radioStarted", "trackStarted", "trackFinished", "skip"].contains(&event_type)
            || batch_id.is_empty()
            || batch_id.len() > 4096
            || batch_id.chars().any(char::is_control)
            || (event_type != "radioStarted" && !Self::valid_track_id(track_id))
            || !played_seconds.is_finite()
            || played_seconds < 0.0
        {
            return Err("Invalid wave feedback".into());
        }
        let mut payload =
            serde_json::json!({"type": event_type, "timestamp": chrono::Utc::now().timestamp()});
        if event_type == "radioStarted" {
            payload["from"] = serde_json::json!("musiczero");
        } else {
            payload["trackId"] = serde_json::json!(track_id);
            if matches!(event_type, "trackFinished" | "skip") {
                payload["totalPlayedSeconds"] = serde_json::json!(played_seconds);
            }
        }
        let result: serde_json::Value = self
            .client
            .post(url)
            .query(&[("batch-id", batch_id)])
            .json(&payload)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if result["result"] != "ok" {
            return Err("Yandex did not accept wave feedback".into());
        }
        Ok(())
    }

    fn valid_track_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id.bytes().all(|b| {
                b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b':' || b == b'.'
            })
    }

    pub async fn get_stream_url(
        &self,
        track_id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if !Self::valid_track_id(track_id) {
            return Err(format!("Invalid track id: {track_id}").into());
        }
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

        let resp = self
            .download_client
            .get(&target.download_info_url)
            .send()
            .await?;
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
        // Validate XML fields to prevent header/signature injection.
        let valid_host = |s: &str| {
            !s.is_empty()
                && s.len() <= 253
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        };
        let valid_path = |s: &str| {
            !s.is_empty()
                && s.len() <= 2048
                && !s.contains(' ')
                && !s.contains('\n')
                && !s.contains('\r')
                && !s.contains('\0')
                && s.starts_with('/')
        };
        let valid_token = |s: &str| {
            !s.is_empty()
                && s.len() <= 512
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        };
        if !valid_host(host) {
            return Err(format!("Некорректный host в XML: {host}").into());
        }
        if !valid_path(path) {
            return Err(format!("Некорректный path в XML: {path}").into());
        }
        if !valid_token(ts) || !valid_token(sign) {
            return Err("Некорректные поля ts/s в XML".into());
        }
        let path_without_slash = path.strip_prefix('/').unwrap_or(path);

        let salt = "XGRlBW9FXlekgbPrr";
        let mut hasher = Md5::new();
        hasher.update(format!("{salt}{path_without_slash}{sign}").as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        Ok(format!("https://{host}/get-mp3/{hash}/{ts}{path}"))
    }

    pub async fn stream_audio(
        &self,
        stream_url: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !stream_url.starts_with("https://") || stream_url.len() > 4096 {
            return Err("Invalid stream URL".into());
        }
        use tokio::io::AsyncWriteExt;
        let mut delay = Duration::from_millis(500);
        for attempt in 1..=4 {
            match self.download_client.get(stream_url).send().await {
                Ok(mut resp) if resp.status().is_success() => {
                    if let Some(len) = resp.content_length() {
                        if len == 0 || len > 512 * 1024 * 1024 {
                            return Err(format!("Некорректный Content-Length: {len}").into());
                        }
                    }
                    let mut stdout = tokio::io::stdout();
                    let mut written: u64 = 0;
                    while let Some(chunk) = resp.chunk().await? {
                        written = written.saturating_add(chunk.len() as u64);
                        if written > 512 * 1024 * 1024 {
                            return Err("Аудиопоток превышает лимит 512 MiB".into());
                        }
                        stdout.write_all(&chunk).await?;
                    }
                    stdout.flush().await?;
                    if written == 0 {
                        return Err("Пустой аудиопоток".into());
                    }
                    return Ok(());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_token_returns_error() {
        assert!(YandexClient::new("bad\r\ntoken").is_err());
    }
    #[tokio::test]
    async fn wave_continuation_and_feedback_use_the_rotor_wire_format() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for index in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let end = loop {
                    let mut block = [0; 1024];
                    let count = socket.read(&mut block).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&block[..count]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < end + length {
                    let mut block = [0; 1024];
                    let count = socket.read(&mut block).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&block[..count]);
                }
                let body = if index == 1 {
                    assert!(
                        headers.starts_with("POST /feedback?batch-id=batch-1 "),
                        "{headers}"
                    );
                    assert!(headers
                        .to_lowercase()
                        .contains("content-type: application/json"));
                    let payload: serde_json::Value = serde_json::from_slice(&bytes[end..]).unwrap();
                    assert_eq!(payload["type"], "skip");
                    assert_eq!(payload["trackId"], "50");
                    assert_eq!(payload["totalPlayedSeconds"], 2.5);
                    assert!(payload["timestamp"].is_i64());
                    r#"{"result":"ok"}"#
                } else {
                    if index == 0 {
                        assert!(headers.starts_with("GET /tracks?settings2=true "));
                    } else {
                        assert!(headers.starts_with("GET /tracks?settings2=true&queue=50 "));
                    }
                    r#"{"result":{"batchId":"batch-1","sequence":[{"track":{"id":"51","title":"Track","artists":[]}}]}}"#
                };
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
        });
        let client = YandexClient::new("test").unwrap();
        let first = client
            .station_tracks(&format!("{base}/tracks"), None)
            .await
            .unwrap();
        assert_eq!(first.batch_id, "batch-1");
        client
            .station_feedback(&format!("{base}/feedback"), "batch-1", "50", "skip", 2.5)
            .await
            .unwrap();
        let next = client
            .station_tracks(&format!("{base}/tracks"), Some("50"))
            .await
            .unwrap();
        assert_eq!(next.sequence[0].track.as_ref().unwrap().id, "51");
        server.join().unwrap();
    }
}
