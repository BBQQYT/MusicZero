pub mod api;
pub mod config;
pub mod control;

use api::YandexClient;
use mcz::mpris::{build_metadata_map, notify_changed, MprisPlayer, MprisRoot, PlayerCommand};
use mcz::playback::spawn_commands;
use mcz::queue::Queue;
use rodio::{Decoder, OutputStream, Sink};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use zbus::connection::Builder;
use zbus::zvariant::Value;

pub async fn run(with_tray: bool, start_paused: bool) -> Result<(), Box<dyn std::error::Error>> {
    let token = config::load_token()?;
    let ym = Arc::new(YandexClient::new(&token));

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&stream_handle)?);
    let active = Arc::new(AtomicBool::new(!start_paused));
    if start_paused {
        sink.pause();
    }

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
        active.clone(),
    );

    if start_paused {
        let mut changed = HashMap::new();
        changed.insert("PlaybackStatus", Value::from("Paused"));
        notify_changed(&conn, changed).await;
        log::info!("YMZ запущен в режиме ожидания (на паузе)");
    }

    #[cfg(all(feature = "tray", target_os = "linux"))]
    if with_tray {
        tokio::spawn(async move {
            if let Err(e) = mcz::tray::run(mcz::tray::TrayConfig {
                id: "ymz",
                title: "YMZ — Яндекс Музыка",
                service: "org.mpris.MediaPlayer2.ymz",
                wave_settings: true,
            })
            .await
            {
                log::warn!("Трей YMZ завершил работу: {e}");
            }
        });
        log::info!("Трей YMZ активирован");
    }

    let shutdown = mcz::shutdown::wait();
    tokio::pin!(shutdown);

    log::info!("Запуск воспроизведения потока «Моя волна»");

    let mut queue = Queue::new();
    'playback: loop {
        while !active.load(Ordering::SeqCst) {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                Some(id) = switch_rx.recv() => {
                    *playlist_id.write().await = id;
                    queue.clear();
                    sink.stop();
                }
                _ = sleep(Duration::from_millis(150)) => {}
            }
        }

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
            while !active.load(Ordering::SeqCst) {
                tokio::select! {
                    _ = &mut shutdown => return Ok(()),
                    Some(id) = switch_rx.recv() => {
                        *playlist_id.write().await = id;
                        queue.clear();
                        sink.stop();
                        continue 'playback;
                    }
                    _ = sleep(Duration::from_millis(150)) => {}
                }
            }

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
            if active.load(Ordering::SeqCst) {
                sink.play();
            } else {
                sink.pause();
            }
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
                        if sink.empty() && active.load(Ordering::SeqCst) {
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
