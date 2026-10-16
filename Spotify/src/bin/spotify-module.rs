#[path = "../api.rs"]
mod api;
#[path = "../auth.rs"]
mod auth;
#[path = "../config.rs"]
mod config;
#[path = "../stream.rs"]
mod stream;

use serde_json::{json, Value};
use std::error::Error;
use std::io::Write;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn arg<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing {name}").into())
}

fn print_json(value: Value) -> Result<()> {
    serde_json::to_writer(std::io::stdout().lock(), &value)?;
    println!();
    Ok(())
}

fn settings_value(settings: &config::Settings) -> Value {
    json!({"settings": {
        "client_id": settings.client_id,
        "redirect_uri": settings.redirect_uri,
        "bitrate": settings.bitrate,
        "explicit": settings.explicit,
    }})
}

async fn token_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

async fn write_audio(id: &str, settings: &config::Settings) -> Result<()> {
    // The librespot session and its downloads live on this runtime; copying
    // blocks on the file stream, so it belongs to the blocking pool.
    let mut audio = stream::audio_stream(id, settings).await?;
    use std::io::Read as _;
    let copy = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut out = std::io::BufWriter::with_capacity(128 * 1024, std::io::stdout().lock());
        let mut buffer = vec![0u8; 32 * 1024];
        loop {
            let count = audio.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            out.write_all(&buffer[..count])?;
        }
        out.flush()
    });
    copy.await
        .map_err(|e| format!("Spotify audio copy failed: {e}"))??;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let command = arg(&args, 1, "command")?;
    if command == "info" {
        return print_json(json!({
            "protocol": 1, "id": "spotify", "name": "Spotify", "default_playlist": "likes"
        }));
    }
    let settings = config::Settings::load()?;
    settings.validate()?;
    if command == "login" {
        return auth::login(&settings, &token_client().await?).await;
    }
    if command == "settings" {
        return print_json(settings_value(&settings));
    }
    if command == "set-setting" {
        let mut updated = settings.clone();
        updated.set(arg(&args, 2, "key")?, arg(&args, 3, "value")?)?;
        return print_json(settings_value(&updated));
    }
    if !auth::token_path().is_file() {
        return Err("Spotify: run `mz login spotify` first".into());
    }
    if command == "audio" {
        return write_audio(arg(&args, 2, "track id")?, &settings).await;
    }
    let mut client = api::Client::new(settings)?;
    match command {
        "playlists" => print_json(client.playlists().await?),
        "tracks" => print_json(client.tracks(arg(&args, 2, "playlist id")?).await?),
        _ => Err(format!("Unknown command: {command}").into()),
    }
}
