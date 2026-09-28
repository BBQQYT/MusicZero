//! youmz — YouTube Music Zero.

pub mod api;
pub mod auth;
pub mod config;
pub mod control;
pub mod decoder;

use crate::decoder::Decoder as SymphoniaDecoder;
use api::{is_bot_check, History, Track, YtClient};
use mcz::mpris::{build_metadata_map, notify_changed, MprisPlayer, MprisRoot, PlayerCommand};
use mcz::playback::spawn_commands;
use mcz::queue::Queue;
use rodio::{OutputStream, Sink};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use zbus::connection::Builder;
use zbus::zvariant::Value;

/// Сколько раз подряд партия может состоять только из уже проигранного,
/// прежде чем мы очистим историю и пройдём радио заново
const EMPTY_BATCHES_BEFORE_RESET: u32 = 3;

/// Учесть неудачу трека. Три неудачи — и трек отправляется в историю
/// (как уже сыгранный), чтобы демон не зацикливался на вечном 403.
fn mark_failure(failures: &mut HashMap<String, u32>, history: &mut History, track: &Track) {
    let n = failures.entry(track.video_id.clone()).or_default();
    *n += 1;
    if *n >= 3 {
        log::warn!(
            "Трек «{} — {}» не удалось проиграть 3 раза — вырезаю из ротации",
            track.artist,
            track.title
        );
        history.push(track.video_id.clone());
    }
}

pub async fn run(with_tray: bool, start_paused: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Arc::new(config::load()?);
    log::info!("Плейлист: {} (RDMM = Мой джем)", cfg.playlist_id);

    // Авторизация: готовый cookie → импорт сессии YouTube Music Desktop →
    // вход по ссылке при первом запуске. Cookie-сессия дополнительно
    // проверяется на авторизованность (иначе YouTube отдаёт не ваш микс).
    let auth = auth::resolve(
        &cfg.proxy,
        cfg.cookie.as_deref(),
        &auth::load_client_id(),
        &auth::load_client_secret(),
    )
    .await?;

    let yt = Arc::new(YtClient::new(cfg.clone(), auth).await);

    log::info!("Клиент rustypipe готов (cookie-сессия)");

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&stream_handle)?);
    let active = Arc::new(AtomicBool::new(!start_paused));
    if start_paused {
        sink.pause();
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<PlayerCommand>();

    let current_title = Arc::new(RwLock::new("Мой джем".to_string()));
    let current_artist = Arc::new(RwLock::new("YouTube Music".to_string()));
    let current_art_url = Arc::new(RwLock::new(String::new()));
    let current_track_id = Arc::new(RwLock::new("0".to_string()));
    let current_duration_us = Arc::new(RwLock::new(0i64));

    // Плейлист можно переключить из трея не перезапуская демон
    let playlist_id = Arc::new(RwLock::new(cfg.playlist_id.clone()));
    let (switch_tx, mut switch_rx) = mpsc::unbounded_channel::<String>();

    // Имя на шине может ещё висеть на прошлом процессе при быстром рестарте
    let mut conn = None;
    for attempt in 0..10 {
        let mpris_player = MprisPlayer {
            cmd_tx: cmd_tx.clone(),
            sink: sink.clone(),
            current_title: current_title.clone(),
            current_artist: current_artist.clone(),
            current_art_url: current_art_url.clone(),
            current_track_id: current_track_id.clone(),
            current_duration_us: current_duration_us.clone(),
            track_url_prefix: "https://www.youtube.com/watch?v=",
            track_path_prefix: "/org/youmz/Track",
        };
        let control = control::YoumzControl {
            yt: yt.as_ref().clone(),
            switch_tx: switch_tx.clone(),
            playlist_id: playlist_id.clone(),
            current_title: current_title.clone(),
            current_artist: current_artist.clone(),
        };

        match Builder::session()?
            .name("org.mpris.MediaPlayer2.youmz")?
            .serve_at(
                "/org/mpris/MediaPlayer2",
                MprisRoot {
                    identity: "YouTube Music Service",
                    mime_types: &["audio/mp4", "audio/mpeg"],
                },
            )?
            .serve_at("/org/mpris/MediaPlayer2", mpris_player)?
            .serve_at("/org/mcz/Control", control)?
            .build()
            .await
        {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) if e.to_string().contains("NameTaken") => {
                log::warn!(
                    "Имя org.mpris.MediaPlayer2.youmz занято, попытка {}...",
                    attempt + 1
                );
                sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
    let conn = conn.ok_or("Не удалось занять D-Bus имя после 10 попыток")?;

    log::info!("D-Bus шина org.mpris.MediaPlayer2.youmz зарегистрирована");

    let skip_flag = Arc::new(AtomicBool::new(false));

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
        log::info!("YouMZ запущен в режиме ожидания (на паузе)");
    }

    #[cfg(all(feature = "tray", target_os = "linux"))]
    if with_tray {
        tokio::spawn(async move {
            if let Err(e) = mcz::tray::run(mcz::tray::TrayConfig {
                id: "youmz",
                title: "YouMZ — YouTube Music",
                service: "org.mpris.MediaPlayer2.youmz",
                wave_settings: false,
            })
            .await
            {
                log::warn!("Трей YouMZ завершил работу: {e}");
            }
        });
        log::info!("Трей YouMZ активирован");
    }

    // Единый сигнал завершения: SIGTERM/SIGINT от systemd (`systemctl stop`)
    // или от Ctrl-C. Канал асинхронный, поэтому цикл воспроизведения реагирует
    // на остановку даже посреди скачивания трека.
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    {
        let shutdown_tx = shutdown_tx.clone();
        tokio::spawn(async move {
            mcz::shutdown::wait().await;
            let _ = shutdown_tx.send(()).await;
        });
    }

    let mut queue = Queue::new();
    // История успешно заигравших треков — чтобы радио не ходило по кругу
    let mut history = History::new(100);
    // Счётчик неудач по трекам: если трек не заиграл с MAX_FAILURES попыток,
    // он отправляется в историю (чёрный список), чтобы не зациклиться на нём
    let mut failures: HashMap<String, u32> = HashMap::new();
    // Экспоненциальная пауза при бот-проверке: лучше ждать, чем штормить
    // запросами и углублять ограничения
    let mut player_backoff = Duration::from_secs(15);
    // Пауза и счётчик, когда партия состоит только из уже проигранного
    let mut empty_backoff = Duration::from_secs(10);
    let mut empty_streak = 0u32;

    log::info!("Запуск воспроизведения «Мой джем»");

    loop {
        while !active.load(Ordering::SeqCst) {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    sink.stop();
                    log::info!("Завершение работы youmz");
                    return Ok(());
                }
                Some(new_id) = switch_rx.recv() => {
                    *playlist_id.write().await = new_id;
                    queue.clear();
                    history.clear();
                }
                _ = sleep(Duration::from_millis(150)) => {}
            }
        }

        // Подгружаем партию треков, когда очередь пуста
        if queue.tracks.is_empty() {
            let current_playlist = playlist_id.read().await.clone();
            let batch = tokio::select! {
                _ = shutdown_rx.recv() => {
                    sink.stop();
                    log::info!("Завершение работы youmz");
                    return Ok(());
                }
                Some(new_id) = switch_rx.recv() => {
                    *playlist_id.write().await = new_id;
                    queue.clear();
                    history.clear();
                    continue;
                }
                res = yt.get_mix_tracks(&current_playlist) => match res {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Ошибка получения микса: {e}. Повтор через 5с");
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                },
            };

            let fresh: Vec<Track> = history.filter_fresh(&batch).into_iter().cloned().collect();

            if fresh.is_empty() {
                empty_streak += 1;
                if empty_streak >= EMPTY_BATCHES_BEFORE_RESET {
                    history.clear();
                    empty_streak = 0;
                    empty_backoff = Duration::from_secs(10);
                    log::info!("Радио пройдено целиком — очищаю историю, начинаю круг заново");
                } else {
                    log::warn!(
                        "Партия — только уже проигранное/рекламное; жду {:.0?} (подряд: {empty_streak})",
                        empty_backoff
                    );
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            sink.stop();
                            log::info!("Завершение работы youmz");
                            return Ok(());
                        }
                        _ = sleep(empty_backoff) => {}
                    }
                    empty_backoff = (empty_backoff * 2).min(Duration::from_secs(300));
                }
                continue;
            }
            empty_streak = 0;
            empty_backoff = Duration::from_secs(10);

            queue.extend(fresh);
        }

        let track = match queue.next() {
            Some(t) => t,
            None => continue,
        };

        while !active.load(Ordering::SeqCst) {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    sink.stop();
                    log::info!("Завершение работы youmz");
                    return Ok(());
                }
                Some(new_id) = switch_rx.recv() => {
                    *playlist_id.write().await = new_id;
                    queue.clear();
                    history.clear();
                    skip_flag.store(false, Ordering::SeqCst);
                    continue;
                }
                _ = sleep(Duration::from_millis(150)) => {}
            }
        }

        let duration_us = track.duration_us;

        // Берём предзагруженные байты или скачиваем через yt-dlp.
        // Скачивание может занимать время (медленный прокси), поэтому
        // оно прерывается по сигналу завершения.
        let fetch_res = tokio::select! {
            _ = shutdown_rx.recv() => {
                sink.stop();
                log::info!("Завершение работы youmz");
                return Ok(());
            }
            Some(new_id) = switch_rx.recv() => {
                *playlist_id.write().await = new_id;
                queue.clear();
                history.clear();
                skip_flag.store(false, Ordering::SeqCst);
                continue;
            }
            res = async {
                if let Some(b) = yt.take_cached(&track.video_id).await {
                    Ok(b)
                } else {
                    yt.fetch_audio(&track.video_id).await
                }
            } => res,
        };

        let bytes = match fetch_res {
            Ok(b) => {
                player_backoff = Duration::from_secs(15);
                b
            }
            Err(e) if is_bot_check(&e) => {
                queue.tracks.push_front(track);
                log::warn!(
                    "Бот-проверка («{e}»): пауза {:.0?}, трек останется в очереди",
                    player_backoff
                );
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        sink.stop();
                        log::info!("Завершение работы youmz");
                        return Ok(());
                    }
                    _ = sleep(player_backoff) => {}
                }
                player_backoff = (player_backoff * 2).min(Duration::from_secs(600));
                continue;
            }
            Err(e) => {
                log::warn!(
                    "Ошибка скачивания «{} — {}»: {e}",
                    track.artist,
                    track.title
                );
                mark_failure(&mut failures, &mut history, &track);
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let source = match SymphoniaDecoder::new(Cursor::new(bytes)) {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "Ошибка декодирования «{} — {}»: {e}",
                    track.artist,
                    track.title
                );
                mark_failure(&mut failures, &mut history, &track);
                continue;
            }
        };

        history.push(track.video_id.clone());
        failures.remove(&track.video_id);

        let art_url = yt
            .fetch_and_cache_cover(&track.video_id, &track.art_url)
            .await
            .unwrap_or_else(|| track.art_url.clone());

        // Публикуем метаданные в MPRIS
        *current_title.write().await = track.title.clone();
        *current_artist.write().await = track.artist.clone();
        *current_track_id.write().await = track.video_id.clone();
        *current_art_url.write().await = art_url.clone();
        *current_duration_us.write().await = duration_us;

        let meta = build_metadata_map(
            &track.title,
            &track.artist,
            &art_url,
            &track.video_id,
            duration_us,
            "https://www.youtube.com/watch?v=",
            "/org/youmz/Track",
        );
        let mut changed = HashMap::new();
        changed.insert("Metadata", Value::from(meta));
        changed.insert("PlaybackStatus", Value::from("Playing"));
        notify_changed(&conn, changed).await;

        log::info!("▶ {} — {}", track.artist, track.title);

        // Предзагружаем следующий трек в фон — скип будет мгновенным
        for next in queue.tracks.iter().take(cfg.prefetch_count) {
            yt.try_preload(next.clone());
        }

        sink.stop();
        sink.append(source);
        if active.load(Ordering::SeqCst) {
            sink.play();
        } else {
            sink.pause();
        }

        // Цикл ожидания конца трека / скипа / завершения
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    sink.stop();
                    log::info!("Завершение работы youmz");
                    return Ok(());
                }
                Some(new_id) = switch_rx.recv() => {
                    {
                        let mut w = playlist_id.write().await;
                        *w = new_id.clone();
                    }
                    queue.clear();
                    history.clear();
                    skip_flag.store(false, Ordering::SeqCst);
                    log::info!("🔀 Переключение на плейлист {new_id}");
                    sink.stop();
                    break;
                }
                _ = sleep(Duration::from_millis(150)) => {
                    if skip_flag.swap(false, Ordering::SeqCst) {
                        log::info!("⏭ Скип: {} — {}", track.artist, track.title);
                        sink.stop();
                        break;
                    }
                    if sink.empty() && active.load(Ordering::SeqCst) {
                        yt.forget(&track.video_id).await;
                        break;
                    }
                }
            }
        }
    }
}

pub async fn login() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let gui_bin = exe.with_file_name("youmz-login");

    let display_ok = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

    if gui_bin.exists() && display_ok {
        println!("Открываю окно входа в YouTube Music…");
        let status = std::process::Command::new(&gui_bin).status()?;
        if status.success() {
            println!("Готово! Теперь можно запускать youmz / mz youmz");
            return Ok(());
        }
        println!("Окно входа закрылось без успеха, пробую вход по ссылке…");
    } else if !gui_bin.exists() {
        log::warn!(
            "Бинарник {} не найден — соберите с фичей gui-login. Использую вход по ссылке.",
            gui_bin.display()
        );
    } else if !display_ok {
        log::warn!("Нет графического дисплея (DISPLAY/WAYLAND_DISPLAY) — вход по ссылке.");
    }

    let proxy = config::load()?.proxy;
    auth::interactive_login(&proxy, &auth::load_client_id(), &auth::load_client_secret()).await?;
    println!("Готово! Cookie сохранён, можно запускать youmz / mz youmz");
    Ok(())
}
