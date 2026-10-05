use base64::Engine;
use mz_module_support::{self as support, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Station {
    id: String,
    name: String,
    url: String,
    username: String,
    password: String,
}
impl Default for Station {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Icecast".into(),
            url: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    station: String,
    stations: Vec<Station>,
    proxy: String,
    user_agent: String,
    reconnect: bool,
    reconnect_delay_max: u32,
    timeout: u32,
    buffer_ms: u32,
    tls_verify: bool,
    ca_file: String,
    ffmpeg: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            station: "default".into(),
            stations: vec![Station::default()],
            proxy: String::new(),
            user_agent: "MusicZero/0.3".into(),
            reconnect: true,
            reconnect_delay_max: 5,
            timeout: 15,
            buffer_ms: 1000,
            tls_verify: true,
            ca_file: String::new(),
            ffmpeg: "ffmpeg".into(),
        }
    }
}
fn url(value: &str) -> Result<Url> {
    support::text(value, 4096)?;
    let url = Url::parse(value)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Нужен HTTP(S) URL; логин/пароль задавайте отдельно".into());
    }
    Ok(url)
}
fn validate(settings: &Settings) -> Result<()> {
    if settings.stations.is_empty() || settings.stations.len() > 200 {
        return Err("Expected 1..200 stations".into());
    }
    let mut ids = std::collections::HashSet::new();
    for station in &settings.stations {
        if station.id.is_empty()
            || station.id.len() > 64
            || !station
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            || !ids.insert(&station.id)
        {
            return Err("Invalid or duplicate station id".into());
        }
        support::text(&station.name, 256)?;
        support::text(&station.username, 256)?;
        support::text(&station.password, 4096)?;
        if station.username.contains(':') {
            return Err("Username cannot contain ':'".into());
        }
        if !station.url.is_empty() {
            url(&station.url)?;
        }
    }
    if !settings.stations.iter().any(|s| s.id == settings.station) {
        return Err("Unknown selected station".into());
    }
    if !(250..=10000).contains(&settings.buffer_ms)
        || !(1..=120).contains(&settings.timeout)
        || !(1..=60).contains(&settings.reconnect_delay_max)
    {
        return Err("buffer_ms: 250..10000; timeout: 1..120; reconnect_delay_max: 1..60".into());
    }
    if !settings.proxy.is_empty() {
        let proxy = url(&settings.proxy)?;
        if proxy.scheme() != "http" {
            return Err("FFmpeg proxy must use http://".into());
        }
    }
    support::text(&settings.user_agent, 1024)?;
    support::text(&settings.ca_file, 4096)?;
    support::text(&settings.ffmpeg, 4096)?;
    Ok(())
}
fn visible(settings: &Settings) -> serde_json::Value {
    let mut value = serde_json::to_value(settings).expect("serializable settings");
    for station in value["stations"].as_array_mut().expect("station array") {
        station["password"] = json!(if station["password"].as_str().unwrap_or("").is_empty() {
            ""
        } else {
            "***"
        });
    }
    json!({"settings":value})
}
fn set(settings: &mut Settings, key: &str, value: &str) -> Result<()> {
    match key {
        "station_add" => {
            settings.stations.push(serde_json::from_str(value)?);
        }
        "station_delete" => {
            if settings.stations.len() <= 1 {
                return Err("Keep at least one station".into());
            }
            if !settings.stations.iter().any(|s| s.id == value) {
                return Err("Unknown station".into());
            }
            settings.stations.retain(|s| s.id != value);
            if settings.station == value {
                settings.station = settings.stations[0].id.clone();
            }
        }
        "station_update" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Update {
                id: String,
                key: String,
                value: String,
            }
            let update: Update = serde_json::from_str(value)?;
            let selected = settings.station.clone();
            if !settings.stations.iter().any(|s| s.id == update.id) {
                return Err("Unknown station".into());
            }
            if update.key == "id" {
                settings
                    .stations
                    .iter_mut()
                    .find(|s| s.id == update.id)
                    .unwrap()
                    .id = update.value.clone();
                if settings.station == update.id {
                    settings.station = update.value;
                }
            } else {
                if ![
                    "name",
                    "url",
                    "server",
                    "mountpoint",
                    "username",
                    "password",
                ]
                .contains(&update.key.as_str())
                {
                    return Err("Unknown station field".into());
                }
                settings.station = update.id;
                let result = set(settings, &update.key, &update.value);
                settings.station = selected;
                result?;
            }
        }
        "station" => settings.station = support::text(value, 64)?,
        "stations" => {
            settings.stations = serde_json::from_str(value)?;
            if !settings.stations.iter().any(|s| s.id == settings.station) {
                settings.station = settings
                    .stations
                    .first()
                    .ok_or("Empty station list")?
                    .id
                    .clone();
            }
        }
        "proxy" => settings.proxy = support::text(value, 4096)?,
        "user_agent" => settings.user_agent = support::text(value, 1024)?,
        "reconnect" => settings.reconnect = support::boolean(value)?,
        "reconnect_delay_max" => settings.reconnect_delay_max = value.parse()?,
        "timeout" => settings.timeout = value.parse()?,
        "buffer_ms" => settings.buffer_ms = value.parse()?,
        "tls_verify" => settings.tls_verify = support::boolean(value)?,
        "ca_file" => settings.ca_file = support::text(value, 4096)?,
        "ffmpeg" => settings.ffmpeg = support::text(value, 4096)?,
        "name" | "url" | "server" | "mountpoint" | "username" | "password" => {
            let station = settings
                .stations
                .iter_mut()
                .find(|s| s.id == settings.station)
                .ok_or("Unknown selected station")?;
            match key {
                "name" => station.name = support::text(value, 256)?,
                "url" => {
                    url(value)?;
                    station.url = value.into();
                }
                "server" => {
                    let mut base = url(value)?;
                    if !station.url.is_empty() {
                        base.set_path(url(&station.url)?.path());
                    }
                    station.url = base.into();
                }
                "mountpoint" => {
                    let mut base = url(&station.url)?;
                    if !value.starts_with('/') || value.contains('?') || value.contains('#') {
                        return Err(
                            "mountpoint must start with '/' and contain no query/fragment".into(),
                        );
                    }
                    base.set_path(&support::text(value, 2048)?);
                    station.url = base.into();
                }
                "username" => station.username = support::text(value, 256)?,
                "password" => station.password = support::text(value, 4096)?,
                _ => unreachable!(),
            }
        }
        _ => return Err(format!("Unknown setting: {key}").into()),
    }
    validate(settings)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    let arg = |n| args.get(n).map(String::as_str).ok_or("Missing argument");
    if command == "info" {
        return support::print_json(
            json!({"protocol":1,"id":"icecast","name":"Icecast Radio","default_playlist":"default"}),
        );
    }
    let mut settings: Settings = support::load("mz-icecast")?;
    validate(&settings)?;
    match command {
        "settings" => support::print_json(visible(&settings)),
        "set-setting" => {
            set(&mut settings, arg(1)?, arg(2)?)?;
            support::save_settings("mz-icecast", &settings)?;
            support::print_json(visible(&settings))
        }
        "playlists" => support::print_json(
            json!({"playlists":settings.stations.iter().map(|s| json!({"id":s.id,"name":s.name})).collect::<Vec<_>>()}),
        ),
        "tracks" => {
            let id = arg(1)?;
            // The default playlist follows the station selected in settings.
            let id = if id == "default" {
                settings.station.as_str()
            } else {
                id
            };
            let station = settings
                .stations
                .iter()
                .find(|s| s.id == id)
                .ok_or("Unknown station")?;
            if station.url.is_empty() {
                return Err(
                    "Укажите станцию: mz set icecast url https://server:port/mountpoint".into(),
                );
            }
            support::print_json(
                json!({"tracks":[{"id":station.id,"title":station.name,"artist":"Icecast Radio","duration_ms":0,"stream":true,"buffer_ms":settings.buffer_ms}]}),
            )
        }
        "audio" => {
            let station = settings
                .stations
                .iter()
                .find(|s| s.id == arg(1).unwrap_or(""))
                .ok_or("Unknown station")?;
            let https = url(&station.url)?.scheme() == "https";
            let mut ffmpeg = Command::new(&settings.ffmpeg);
            ffmpeg
                .args([
                    "-nostdin",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-protocol_whitelist",
                    "http,https,tcp,tls,crypto",
                    "-rw_timeout",
                ])
                .arg((settings.timeout as u64 * 1_000_000).to_string())
                .arg("-user_agent")
                .arg(&settings.user_agent);
            if https {
                ffmpeg
                    .arg("-tls_verify")
                    .arg(if settings.tls_verify { "1" } else { "0" });
                if !settings.ca_file.is_empty() {
                    ffmpeg.arg("-ca_file").arg(&settings.ca_file);
                }
            }
            if !settings.proxy.is_empty() {
                ffmpeg.arg("-http_proxy").arg(&settings.proxy);
            }
            if settings.reconnect {
                ffmpeg
                    .args([
                        "-reconnect",
                        "1",
                        "-reconnect_at_eof",
                        "1",
                        "-reconnect_streamed",
                        "1",
                        "-reconnect_on_network_error",
                        "1",
                        "-reconnect_on_http_error",
                        "429,5xx",
                        "-reconnect_delay_max",
                    ])
                    .arg(settings.reconnect_delay_max.to_string());
            }
            if !station.username.is_empty() || !station.password.is_empty() {
                let credentials = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", station.username, station.password));
                ffmpeg
                    .arg("-headers")
                    .arg(format!("Authorization: Basic {credentials}\r\n"));
            }
            ffmpeg.arg("-i").arg(&station.url).args([
                "-map",
                "0:a:0",
                "-vn",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-c:a",
                "pcm_s16le",
                "-f",
                "s16le",
                "pipe:1",
            ]);
            support::run_ffmpeg(ffmpeg)
        }
        "login" => {
            eprintln!("Icecast: настройте URL и при необходимости username/password через mz set icecast ...");
            Ok(())
        }
        _ => Err("Unknown Icecast command".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_redacted_and_settings_reject_header_injection() {
        let mut settings = Settings::default();
        set(&mut settings, "password", "secret").unwrap();
        assert!(!visible(&settings).to_string().contains("secret"));
        assert!(set(&mut settings, "username", "a\r\nInjected: bad").is_err());
        assert!(set(&mut settings, "user_agent", "a\nInjected: bad").is_err());
        assert!(url("file:///etc/passwd").is_err());
        assert!(url("https://example.com/\nInjected").is_err());
    }
    #[test]
    fn station_mount_and_bounds_are_validated() {
        let mut settings = Settings::default();
        set(&mut settings, "url", "https://example.com:8443/old").unwrap();
        set(&mut settings, "mountpoint", "/radio.ogg").unwrap();
        assert_eq!(
            settings.stations[0].url,
            "https://example.com:8443/radio.ogg"
        );
        assert!(set(&mut settings, "buffer_ms", "999999999").is_err());
        assert!(set(&mut settings, "stations", r#"[{"id":"x"},{"id":"x"}]"#).is_err());
    }
}
