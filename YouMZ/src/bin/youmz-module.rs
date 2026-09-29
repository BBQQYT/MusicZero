#[path = "../api.rs"]
mod api;
#[path = "../auth.rs"]
mod auth;
#[path = "../config.rs"]
mod config;
#[path = "../paths.rs"]
mod paths;

use api::YtClient;
use serde_json::json;
use std::error::Error;
use std::io::{self, Write};
use std::sync::Arc;

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
        match auth::interactive_login(&cfg.proxy, &auth::load_client_id(), &auth::load_client_secret()).await? {
            auth::Auth::Cookie(_) => {},
            auth::Auth::Bearer(_) => return Err("Не удалось создать cookie-сессию. Сохраните cookie браузера в конфиг YouMZ или задайте YOUMZ_COOKIE.".into()),
        }
        return Ok(());
    }
    if command == "settings" {
        return print_json(json!({"settings": {}}));
    }
    let cookie = cfg
        .cookie
        .clone()
        .ok_or("YouTube Music: run `mz login youmz` first")?;
    let yt = YtClient::new(cfg.clone(), auth::Auth::Cookie(cookie)).await;
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
        "audio" => {
            let id = arg(&args, 2, "track id")?;
            let bytes = yt.fetch_audio(id).await?;
            io::stdout().lock().write_all(&bytes)?;
            Ok(())
        }
        _ => Err(format!("Unknown command: {command}").into()),
    }
}
