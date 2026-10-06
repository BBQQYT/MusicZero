#[path = "../api.rs"]
mod api;
#[path = "../config.rs"]
mod config;
#[path = "../paths.rs"]
mod paths;

use api::YandexClient;
use serde_json::json;
use std::error::Error;
use std::io::{self, Write};
use std::process::Command;

const TOKEN_URL: &str = "https://ym-token.marshal.dev/";

fn open_token_page() -> io::Result<()> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "start", "", TOKEN_URL])
            .spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let mut last_error = None;
        for browser in [
            "termux-open-url",
            "xdg-open",
            "firefox",
            "librewolf",
            "chromium",
            "google-chrome",
            "chrome",
        ] {
            match Command::new(browser).arg(TOKEN_URL).spawn() {
                Ok(_) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Браузер не найден")))
    }
}

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn text<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if std::env::var("MZ_LANGUAGE").as_deref() == Ok("en") {
        en
    } else {
        ru
    }
}

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

async fn settings() -> Result<serde_json::Value> {
    let token = config::load_token();
    let mut value = json!({"settings":{"token":if token.is_ok() { "***" } else { "" },
        "mood":null,"diversity":null,"language":null}});
    let wave = async { YandexClient::new(&token?)?.get_wave_settings().await }.await;
    match wave {
        Ok(wave) => {
            value["settings"]["mood"] = json!(wave.mood);
            value["settings"]["diversity"] = json!(wave.diversity);
            value["settings"]["language"] = json!(wave.language);
        }
        Err(error) => value["warning"] = json!(error.to_string()),
    }
    Ok(value)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    match arg(&args, 1, "command")? {
        "info" => print_json(
            json!({"protocol": 1, "id": "ymz", "name": "Яндекс Музыка", "default_playlist": "wave"}),
        ),
        "settings" => print_json(settings().await?),
        "set-setting" if args.get(2).is_some_and(|key| key == "token") => {
            let token = arg(&args, 3, "token")?.trim();
            YandexClient::new(token)?.list_playlists().await?;
            config::save_token(token)?;
            print_json(settings().await?)
        }
        "login" => {
            if let Err(error) = open_token_page() {
                eprintln!("Не удалось открыть браузер: {error}");
            }
            eprintln!(
                "{} {TOKEN_URL}",
                text("Получите токен на", "Get a token at")
            );
            eprint!(
                "{}",
                text(
                    "Вставьте токен Яндекс Музыки и нажмите Enter: ",
                    "Paste your Yandex Music token and press Enter: "
                )
            );
            io::stderr().flush()?;
            let mut token = String::new();
            if io::stdin().read_line(&mut token)? == 0 {
                return Err("Ввод токена отменён".into());
            }
            let token = token.trim();
            if token.is_empty() || token.chars().any(char::is_control) {
                return Err("Токен пуст или содержит управляющие символы".into());
            }
            YandexClient::new(token)?
                .list_playlists()
                .await
                .map_err(|error| format!("Токен не принят Яндекс Музыкой: {error}"))?;
            config::save_token(token)?;
            eprintln!(
                "Токен сохранён в {}",
                paths::config_dir("ymz").join("token").display()
            );
            Ok(())
        }
        command => {
            let token = config::load_token()?;
            let client = YandexClient::new(&token)?;
            match command {
                "playlists" => {
                    let playlists = client
                        .list_playlists()
                        .await?
                        .into_iter()
                        .map(|(id, name)| json!({"id": id, "name": name}))
                        .collect::<Vec<_>>();
                    print_json(json!({"playlists": playlists}))
                }
                "tracks" => {
                    let selected = arg(&args, 2, "playlist id")?;
                    let mut feedback = String::new();
                    let entries = if selected == "wave" {
                        let after = args.get(3).map(String::as_str);
                        let result = client.get_wave_tracks(after).await?;
                        feedback = result.batch_id;
                        if after.is_none() {
                            if let Err(error) = client
                                .send_feedback(&feedback, "", "radioStarted", 0.0)
                                .await
                            {
                                log::warn!("Wave radioStarted: {error}");
                            }
                        }
                        result.sequence
                    } else {
                        let kind = selected
                            .strip_prefix("playlist:")
                            .ok_or("Invalid playlist id")?
                            .parse::<u64>()?;
                        client.get_playlist_tracks(kind).await?
                    };
                    let tracks = entries
                        .into_iter()
                        .filter_map(|entry| entry.track)
                        .map(|track| {
                            let artist =
                                track.artists.first().map(|a| a.name.as_str()).unwrap_or("");
                            let art_url = track
                                .cover_uri
                                .as_deref()
                                .map(|uri| format!("https://{}", uri.replace("%%", "400x400")))
                                .unwrap_or_default();
                            json!({"id": track.id, "title": track.title, "artist": artist,
                            "art_url": art_url, "duration_ms": track.duration_ms.unwrap_or(0), "feedback":feedback})
                        })
                        .collect::<Vec<_>>();
                    print_json(json!({"tracks": tracks, "continuous":selected == "wave"}))
                }
                "feedback" => {
                    let event = arg(&args, 2, "feedback event")?;
                    let track_id = arg(&args, 3, "track id")?;
                    let batch_id = arg(&args, 4, "batch id")?;
                    let played = arg(&args, 5, "played seconds")?.parse::<f64>()?;
                    client
                        .send_feedback(batch_id, track_id, event, played)
                        .await?;
                    print_json(json!({"ok":true}))
                }
                "audio" => {
                    let id = arg(&args, 2, "track id")?;
                    let url = client.get_stream_url(id).await?;
                    client.stream_audio(&url).await
                }
                "set-setting" => {
                    let key = arg(&args, 2, "setting key")?;
                    let value = arg(&args, 3, "setting value")?;
                    let mut wave = client.get_wave_settings().await?;
                    match key {
                        "mood" => wave.mood = value.into(),
                        "diversity" => wave.diversity = value.into(),
                        "language" => wave.language = value.into(),
                        _ => return Err(format!("Unknown setting: {key}").into()),
                    }
                    client.set_wave_settings(&wave).await?;
                    print_json(json!({"settings": {"mood": wave.mood,
                        "diversity": wave.diversity, "language": wave.language,"token":"***"}}))
                }
                _ => Err(format!("Unknown command: {command}").into()),
            }
        }
    }
}
