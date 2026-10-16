use crate::{auth::Tokens, config::Settings, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Client {
    http: reqwest::Client,
    tokens: Tokens,
    settings: Settings,
    base: url::Url,
}

impl Client {
    pub fn new(settings: Settings) -> Result<Self> {
        settings.validate()?;
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            tokens: Tokens::load()?,
            settings,
            base: url::Url::parse("https://api.spotify.com/v1/")?,
        })
    }

    async fn request(&mut self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let url = self.base.join(path)?;
        if url.origin() != self.base.origin() || !url.path().starts_with(self.base.path()) {
            return Err("Spotify pagination URL escaped the API origin".into());
        }
        for attempt in 0..3 {
            let token = self.tokens.fresh(&self.http, &self.settings).await?;
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(token);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await?;
            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.tokens.expires_at = 0;
                continue;
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1);
                if delay > 20 {
                    return Err(format!("Spotify rate limit; retry in {delay} seconds").into());
                }
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !status.is_success() {
                return Err(format!("Spotify API HTTP {status}: check login, Premium, app allowlist and endpoint access").into());
            }
            if status == reqwest::StatusCode::NO_CONTENT {
                return Ok(json!({"ok":true}));
            }
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                    return Err("Spotify response exceeds 4 MiB".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            return if bytes.is_empty() {
                Ok(json!({"ok":true}))
            } else {
                Ok(serde_json::from_slice(&bytes)?)
            };
        }
        Err("Spotify request retry limit exceeded".into())
    }

    async fn pages(&mut self, path: &str) -> Result<Vec<Value>> {
        let mut next = Some(path.to_owned());
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while let Some(path) = next.take() {
            if !seen.insert(path.clone()) {
                return Err("Spotify pagination loop detected".into());
            }
            let page = self.request(Method::GET, &path, None).await?;
            let batch = page["items"]
                .as_array()
                .ok_or("Missing Spotify page items")?;
            items.extend(batch.iter().cloned());
            if items.len() > 100_000 {
                return Err("Spotify library exceeds 100000 items".into());
            }
            next = page["next"].as_str().map(str::to_owned);
        }
        Ok(items)
    }

    pub async fn playlists(&mut self) -> Result<Value> {
        let mut playlists = vec![
            json!({"id":"likes","name":"Мне нравится / Liked Songs"}),
            json!({"id":"recent","name":"Недавно прослушанное / Recently played"}),
            json!({"id":"top","name":"Любимые треки / Top tracks"}),
        ];
        playlists.extend(self.pages("me/playlists?limit=50").await?.iter().filter_map(|p| {
            Some(json!({"id":format!("playlist:{}",p["id"].as_str()?),"name":p["name"].as_str()?}))
        }));
        playlists.extend(self.pages("me/albums?limit=50").await?.iter().filter_map(|p| {
            let album=&p["album"];
            Some(json!({"id":format!("album:{}",album["id"].as_str()?),"name":format!("Album: {}",album["name"].as_str()?)}))
        }));
        Ok(json!({"playlists":playlists}))
    }

    pub async fn tracks(&mut self, playlist: &str) -> Result<Value> {
        let (path, wrapped) =
            match playlist {
                "likes" => ("me/tracks?limit=50".to_owned(), true),
                "recent" => ("me/player/recently-played?limit=50".to_owned(), true),
                "top" => ("me/top/tracks?limit=50".to_owned(), false),
                value if value.starts_with("playlist:") => {
                    let id = spotify_id(&value[9..])?;
                    (format!("playlists/{id}/items?limit=50"), true)
                }
                value if value.starts_with("album:") => {
                    let id = spotify_id(&value[6..])?;
                    (format!("albums/{id}/tracks?limit=50"), false)
                }
                _ => return Err(
                    "Invalid Spotify playlist; use likes, recent, top, playlist:<id> or album:<id>"
                        .into(),
                ),
            };
        let mut entries = self.pages(&path).await?;
        let album = match playlist.strip_prefix("album:") {
            Some(album_id) => Some(
                self.request(Method::GET, &format!("albums/{album_id}"), None)
                    .await?,
            ),
            None => None,
        };
        let tracks = entries
            .iter_mut()
            .filter_map(|entry| {
                // Spotify's 2026 playlist endpoint uses item; older library endpoints use track.
                let item = if wrapped {
                    if entry.get("item").is_some() {
                        &mut entry["item"]
                    } else {
                        &mut entry["track"]
                    }
                } else {
                    entry
                };
                if let Some(album) = &album {
                    item["album"] = album.clone();
                }
                host_track(item, self.settings.explicit)
            })
            .collect::<Vec<_>>();
        Ok(json!({"tracks":tracks}))
    }
}

pub fn spotify_id(value: &str) -> Result<&str> {
    if value.len() != 22 || !value.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Spotify IDs must contain 22 ASCII letters/digits".into());
    }
    Ok(value)
}
pub fn spotify_uri(value: &str) -> Result<&str> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != "spotify"
        || !["track", "episode", "album", "playlist", "artist", "show"].contains(&parts[1])
    {
        return Err("Invalid Spotify URI".into());
    }
    spotify_id(parts[2])?;
    Ok(value)
}
fn host_track(item: &Value, explicit: bool) -> Option<Value> {
    if item.is_null()
        || item["is_local"].as_bool() == Some(true)
        || item["is_playable"].as_bool() == Some(false)
        || (!explicit && item["explicit"].as_bool() == Some(true))
    {
        return None;
    }
    let id = item["id"].as_str()?;
    spotify_id(id).ok()?;
    let kind = item["type"].as_str().unwrap_or("track");
    if !["track", "episode"].contains(&kind) {
        return None;
    }
    let artist = if kind == "episode" {
        item["show"]["name"].as_str().unwrap_or("").to_owned()
    } else {
        item["artists"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|a| a["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let images = if kind == "episode" {
        &item["images"]
    } else {
        &item["album"]["images"]
    };
    Some(
        json!({"id":format!("{kind}:{id}"),"title":item["name"].as_str()?,"artist":artist,
        "art_url":images[0]["url"].as_str().unwrap_or(""),"duration_ms":item["duration_ms"].as_i64().unwrap_or(0)}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_keeps_artists_and_filters_unplayable_content() {
        let mut track = json!({"id":"0123456789012345678901","type":"track","name":"Test",
            "artists":[{"name":"First"},{"name":"Second"}],"album":{"images":[{"url":"https://example.org/art"}]},"duration_ms":1234});
        let metadata = host_track(&track, true).unwrap();
        assert_eq!(metadata["artist"], "First, Second");
        assert_eq!(metadata["duration_ms"], 1234);
        track["explicit"] = json!(true);
        assert!(host_track(&track, false).is_none());
        track["is_playable"] = json!(false);
        assert!(host_track(&track, true).is_none());
        assert!(spotify_uri("spotify:track:../../secret").is_err());
    }
}
