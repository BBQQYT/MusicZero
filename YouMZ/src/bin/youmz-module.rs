#[path = "../api.rs"]
mod api;
#[path = "../auth.rs"]
mod auth;
#[path = "../browser_login.rs"]
mod browser_login;
#[path = "../config.rs"]
mod config;
#[path = "../paths.rs"]
mod paths;

use api::YtClient;
use serde_json::json;
use std::error::Error;
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn arg<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing {name}").into())
}

fn print_json(value: serde_json::Value) -> Result<()> {
    serde_json::to_writer(io::stdout().lock(), &value)?;
    println!();
    Ok(())
}

fn valid_video_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

async fn stream_audio(video_id: &str, session: &str, proxy: Option<&str>) -> Result<()> {
    if !valid_video_id(video_id) {
        return Err(format!("Invalid video id: {video_id}").into());
    }
    if session.is_empty()
        || session.len() > 32 * 1024
        || session
            .chars()
            .any(|c| c.is_control() || c == '\n' || c == '\r')
    {
        return Err("Invalid YouTube session".into());
    }
    if let Some(proxy) = proxy {
        if proxy.len() > 1024 || proxy.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err("Invalid proxy".into());
        }
    }
    // yt-dlp reads config from stdin (`--config-locations -`). The Cookie header is
    // injected as a single config directive; session is already filtered for control
    // chars and newlines above so no directive injection is possible.
    let mut cmd = tokio::process::Command::new("yt-dlp");
    cmd.arg("--ignore-config")
        .arg("--config-locations")
        .arg("-")
        .arg("-f")
        .arg("140/ba/bestaudio")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--no-progress")
        .arg("-o")
        .arg("-")
        .arg(format!("https://www.youtube.com/watch?v={video_id}"));
    if let Some(proxy) = proxy {
        cmd.arg("--proxy").arg(proxy);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    // Escape backslash and double-quote for the quoted config value.
    let escaped = session.replace('\\', "\\\\").replace('"', "\\\"");
    let header = format!("--add-headers \"Cookie:{escaped}\"\n");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(header.as_bytes()).await?;
        // Close stdin so yt-dlp sees EOF on config; drop explicitly.
    }
    let status = tokio::time::timeout(std::time::Duration::from_secs(300), child.wait())
        .await
        .map_err(|_| "yt-dlp timed out after 300s".to_string())??;
    if status.success() {
        Ok(())
    } else {
        Err(format!("yt-dlp exited: {status}").into())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let command = arg(&args, 1, "command")?;
    if command == "info" {
        return print_json(
            json!({"protocol": 1, "id": "youmz", "name": "YouTube Music", "default_playlist": "RDMM"}),
        );
    }
    let cfg = Arc::new(config::load()?);
    if command == "login" {
        let session = browser_login::login(&cfg.proxy).await?;
        auth::save_session(&session)?;
        eprintln!("Вход в YouTube Music завершён. Сессия сохранена в конфиге YouMZ.");
        return Ok(());
    }
    if command == "settings" {
        return print_json(json!({"settings": {}}));
    }
    let cookie = cfg
        .cookie
        .clone()
        .ok_or("YouTube Music: run `mz login youmz` first")?;
    if command == "audio" {
        return stream_audio(arg(&args, 2, "track id")?, &cookie, cfg.proxy.as_deref()).await;
    }
    let yt = YtClient::new(cfg.clone()).await?;
    match command {
        "playlists" => {
            let mut playlists = vec![json!({"id": "RDMM", "name": "Мой джем"})];
            for item in yt.list_playlists().await? {
                if !playlists.iter().any(|p| p["id"] == item.id) {
                    playlists.push(json!({"id": item.id, "name": item.title}));
                }
            }
            print_json(json!({"playlists": playlists}))
        }
        "tracks" => {
            let playlist = arg(&args, 2, "playlist id")?;
            let tracks = yt
                .get_mix_tracks(playlist)
                .await?
                .into_iter()
                .map(|track| {
                    json!({"id": track.video_id, "title": track.title, "artist": track.artist,
                    "art_url": track.art_url, "duration_ms": track.duration_us / 1000})
                })
                .collect::<Vec<_>>();
            print_json(json!({"tracks": tracks}))
        }
        _ => Err(format!("Unknown command: {command}").into()),
    }
}
