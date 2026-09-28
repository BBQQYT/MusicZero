mod api;
mod config;
mod control;

use api::YandexClient;
use mcz::mpris::{build_metadata_map, notify_changed, MprisPlayer, MprisRoot, PlayerCommand};
use mcz::queue::Queue;
use rodio::{Decoder, OutputStream, Sink};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mcz::playback::spawn_commands;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use zbus::connection::Builder;
use zbus::zvariant::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "help" | "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "version" | "--version" | "-V" => {
                println!("ymz {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            unknown => {
                eprintln!("Неизвестная команда или флаг: {unknown}\n");
                print_help();
                std::process::exit(1);
            }
        }
    }

    let token = config::load_token()?;
    let ym = Arc::new(YandexClient::new(&token));

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&stream_handle)?);

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<PlayerCommand>();

    let current_title = Arc::new(RwLock::new("Моя волна".to_string()));
    let current_artist = Arc::new(RwLock::new("Яндекс Музыка".to_string()));
    let current_art_url = Arc::new(RwLock::new("".to_string()));
    let current_track_id = Arc::new(RwLock::new("0".to_string()));
    let current_duration_us = Arc::new(RwLock::new(0i64));

    let mpris_player = MprisPlayer {
        cmd_tx: cmd_tx.clone(),
        sink: sink.clone(),
        current_title: current_title.clone(),
        current_artist: current_artist.clone(),
        current_art_url: current_art_url.clone(),
        current_track_id: current_track_id.clone(),
        current_duration_us: current_duration_us.clone(),
        track_url_prefix: "https://music.yandex.ru/album/0/track/",
        track_path_prefix: "/org/ymz/Track",
    };

    let conn = Builder::session()?
        .name("org.mpris.MediaPlayer2.ymz")?
        .serve_at(
            "/org/mpris/MediaPlayer2",
            MprisRoot {
                identity: "Yandex Music Zero",
                mime_types: &["audio/mpeg"],
            },
        )?
        .serve_at("/org/mpris/MediaPlayer2", mpris_player)?
        .build()
        .await?;

    log::info!("D-Bus шина org.mpris.MediaPlayer2.ymz зарегистрирована");

    let skip_flag = Arc::new(AtomicBool::new(false));
    let playlist_id = Arc::new(RwLock::new(config::load_playlist()));
    let wave = Arc::new(RwLock::new(
        ym.get_wave_settings().await.unwrap_or_default(),
    ));
    let (switch_tx, mut switch_rx) = mpsc::unbounded_channel::<String>();
    conn.object_server()
        .at(
            "/org/mcz/Control",
            control::YmzControl {
                ym: ym.clone(),
                switch_tx,
                playlist_id: playlist_id.clone(),
                wave,
                current_title: current_title.clone(),
                current_artist: current_artist.clone(),
            },
        )
        .await?;

    spawn_commands(
        cmd_rx,
        sink.clone(),
        skip_flag.clone(),
        current_duration_us.clone(),
        conn.clone(),
    );

    let shutdown = mcz::shutdown::wait();
    tokio::pin!(shutdown);

    log::info!("Запуск воспроизведения потока «Моя волна»");

    let mut queue = Queue::new();
    'playback: loop {
        while let Ok(id) = switch_rx.try_recv() {
            *playlist_id.write().await = id;
            queue.clear();
            sink.stop();
        }
        let selected = playlist_id.read().await.clone();
        let entries = tokio::select! {
            _ = &mut shutdown => break,
            Some(id) = switch_rx.recv() => {
                *playlist_id.write().await = id;
                queue.clear();
                sink.stop();
                continue;
            }
            res = async {
                if selected == "wave" {
                    ym.get_wave_tracks().await.map(|s| (s.sequence, Some(s.batch_id)))
                } else {
                    let kind = selected.strip_prefix("playlist:").and_then(|s| s.parse::<u64>().ok())
                        .ok_or_else(|| "Некорректный ID плейлиста".to_string())?;
                    ym.get_playlist_tracks(kind).await.map(|v| (v, None))
                }.map_err(|e| e.to_string())
            } => match res {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Ошибка получения треков: {e}");
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }
        };

        queue.extend(entries.0);
        while let Some(entry) = queue.next() {
            if let Ok(id) = switch_rx.try_recv() {
                *playlist_id.write().await = id;
                queue.clear();
                sink.stop();
                continue 'playback;
            }
            let Some(track) = entry.track else { continue };
            let artist_name = track
                .artists
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Неизвестный исполнитель".to_string());

            let cover_url = track
                .cover_uri
                .as_ref()
                .map(|uri| format!("https://{}", uri.replace("%%", "400x400")))
                .unwrap_or_default();

            let duration_us = track.duration_ms.unwrap_or(0) * 1000;

            let stream_url = tokio::select! {
                _ = &mut shutdown => return Ok(()),
                Some(id) = switch_rx.recv() => {
                    *playlist_id.write().await = id;
                    queue.clear();
                    sink.stop();
                    continue 'playback;
                }
                result = ym.get_stream_url(&track.id) => match result {
                    Ok(url) => url,
                    Err(e) => { log::warn!("Ошибка получения потока: {e}"); continue; }
                }
            };
            let bytes = tokio::select! {
                _ = &mut shutdown => return Ok(()),
                Some(id) = switch_rx.recv() => {
                    *playlist_id.write().await = id;
                    queue.clear();
                    sink.stop();
                    continue 'playback;
                }
                result = ym.download_audio(&stream_url) => match result {
                    Ok(bytes) => bytes,
                    Err(e) => { log::warn!("Ошибка загрузки аудио: {e}"); continue; }
                }
            };
            let source = match Decoder::new(Cursor::new(bytes)) {
                Ok(source) => source,
                Err(e) => {
                    log::warn!("Ошибка декодирования: {e}");
                    continue;
                }
            };

            *current_title.write().await = track.title.clone();
            *current_artist.write().await = artist_name.clone();
            *current_track_id.write().await = track.id.clone();
            *current_art_url.write().await = cover_url.clone();
            *current_duration_us.write().await = duration_us;

            let meta = build_metadata_map(
                &track.title,
                &artist_name,
                &cover_url,
                &track.id,
                duration_us,
                "https://music.yandex.ru/album/0/track/",
                "/org/ymz/Track",
            );
            let mut changed = HashMap::new();
            changed.insert("Metadata", Value::from(meta));
            changed.insert("PlaybackStatus", Value::from("Playing"));
            notify_changed(&conn, changed).await;

            log::info!("▶ {} — {}", artist_name, track.title);
            skip_flag.store(false, Ordering::SeqCst);
            sink.stop();
            sink.append(source);
            sink.play();
            if let Some(batch) = &entries.1 {
                ym.send_feedback(batch, &track.id, "trackStarted").await;
            }
            loop {
                tokio::select! {
                    _ = &mut shutdown => return Ok(()),
                    Some(id) = switch_rx.recv() => {
                        *playlist_id.write().await = id;
                        queue.clear();
                        sink.stop();
                        continue 'playback;
                    },
                    _ = sleep(Duration::from_millis(150)) => {
                        if skip_flag.swap(false, Ordering::SeqCst) {
                            if let Some(batch) = &entries.1 { ym.send_feedback(batch, &track.id, "skip").await; }
                            break;
                        }
                        if sink.empty() {
                            if let Some(batch) = &entries.1 { ym.send_feedback(batch, &track.id, "trackFinished").await; }
                            break;
                        }
                    }
                }
            }
        }
    }

    log::info!("Завершение работы ymz");
    Ok(())
}

fn print_help() {
    println!(
        "YMZ v{} — легковесный headless-клиент Яндекс Музыки (MPRIS v2)\n\n\
Использование:\n  \
  ymz [КОМАНДА]\n\n\
Команды:\n  \
  (без аргументов)    Запустить плеер (фоновый демон)\n  \
  help, --help, -h    Показать эту справку\n  \
  version, --version  Показать версию\n\n\
Управление воспроизведением:\n  \
  playerctl -p ymz play-pause\n  \
  playerctl -p ymz next\n  \
  playerctl -p ymz previous\n  \
  или через трей: `ymz-tray`",
        env!("CARGO_PKG_VERSION")
    );
}
