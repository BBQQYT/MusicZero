use crate::ipc::ControlRequest;
use crate::plugin::{self, Module, Track};
use mcz::mpris::{build_metadata_map, notify_changed, MprisPlayer, MprisRoot, PlayerCommand};
use rodio::{Decoder, Sink};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::io::BufReader;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tokio::sync::{mpsc, RwLock};
use zbus::connection::Builder;
use zbus::zvariant::Value as BusValue;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct Player {
    modules: Vec<Module>,
    selected: usize,
    playlist: String,
    queue: VecDeque<Track>,
    current: Option<Track>,
    current_file: Option<NamedTempFile>,
    sink: Arc<Sink>,
    active: bool,
    retry_at: Instant,
    mpris: Option<zbus::Connection>,
    title: Arc<RwLock<String>>,
    artist: Arc<RwLock<String>>,
    art_url: Arc<RwLock<String>>,
    track_id: Arc<RwLock<String>>,
    duration_us: Arc<RwLock<i64>>,
}

impl Player {
    async fn new(
        modules: Vec<Module>,
        selected: usize,
        sink: Arc<Sink>,
    ) -> (Self, mpsc::UnboundedReceiver<PlayerCommand>) {
        let title = Arc::new(RwLock::new(String::new()));
        let artist = Arc::new(RwLock::new(String::new()));
        let art_url = Arc::new(RwLock::new(String::new()));
        let track_id = Arc::new(RwLock::new(String::new()));
        let duration_us = Arc::new(RwLock::new(0i64));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let mpris = if cfg!(target_os = "linux") {
            let player = MprisPlayer {
                cmd_tx,
                sink: sink.clone(),
                current_title: title.clone(),
                current_artist: artist.clone(),
                current_art_url: art_url.clone(),
                current_track_id: track_id.clone(),
                current_duration_us: duration_us.clone(),
                track_url_prefix: "",
                track_path_prefix: "/org/mz/Track",
            };
            match async {
                Ok::<_, zbus::Error>(
                    Builder::session()?
                        .name("org.mpris.MediaPlayer2.mz")?
                        .serve_at(
                            "/org/mpris/MediaPlayer2",
                            MprisRoot {
                                identity: "MusicZero",
                                mime_types: &["audio/mpeg", "audio/mp4"],
                            },
                        )?
                        .serve_at("/org/mpris/MediaPlayer2", player)?
                        .build()
                        .await?,
                )
            }
            .await
            {
                Ok(connection) => Some(connection),
                Err(error) => {
                    log::warn!("MPRIS: {error}");
                    None
                }
            }
        } else {
            None
        };
        let playlist = plugin::selected_playlist(&modules[selected]);
        (
            Self {
                modules,
                selected,
                playlist,
                queue: VecDeque::new(),
                current: None,
                current_file: None,
                sink,
                active: true,
                retry_at: Instant::now(),
                mpris,
                title,
                artist,
                art_url,
                track_id,
                duration_us,
            },
            cmd_rx,
        )
    }

    fn module(&self) -> &Module {
        &self.modules[self.selected]
    }

    fn status(&self) -> Value {
        let status = if !self.active {
            "Paused"
        } else if self.current.is_some() && !self.sink.empty() {
            "Playing"
        } else {
            "Loading"
        };
        json!({"module": self.module().manifest.id, "name": self.module().manifest.name,
            "playlist": self.playlist, "status": status,
            "title": self.current.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
            "artist": self.current.as_ref().map(|t| t.artist.as_str()).unwrap_or("")})
    }

    fn clear_current(&mut self) {
        self.sink.stop();
        self.current = None;
        self.current_file = None;
    }

    async fn notify_status(&self, status: &str) {
        if let Some(connection) = &self.mpris {
            let mut changed = HashMap::new();
            changed.insert("PlaybackStatus", BusValue::from(status));
            notify_changed(connection, changed).await;
        }
    }

    async fn handle_mpris(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Play => {
                self.active = true;
                self.sink.play();
                self.notify_status("Playing").await;
            }
            PlayerCommand::Pause => {
                self.active = false;
                self.sink.pause();
                self.notify_status("Paused").await;
            }
            PlayerCommand::PlayPause => {
                self.active = !self.active;
                if self.active {
                    self.sink.play();
                    self.notify_status("Playing").await;
                } else {
                    self.sink.pause();
                    self.notify_status("Paused").await;
                }
            }
            PlayerCommand::Next => {
                self.active = true;
                self.clear_current();
            }
            PlayerCommand::Stop => {
                self.active = false;
                self.clear_current();
                self.notify_status("Stopped").await;
            }
            PlayerCommand::Seek(offset) => {
                let total = *self.duration_us.read().await;
                let target = (self.sink.get_pos().as_micros() as i64 + offset).clamp(0, total);
                if self
                    .sink
                    .try_seek(Duration::from_micros(target as u64))
                    .is_ok()
                {
                    if let Some(connection) = &self.mpris {
                        mcz::mpris::notify_seeked(connection, target).await;
                    }
                }
            }
            PlayerCommand::SetPosition(position) => {
                let total = *self.duration_us.read().await;
                let target = position.clamp(0, total);
                if self
                    .sink
                    .try_seek(Duration::from_micros(target as u64))
                    .is_ok()
                {
                    if let Some(connection) = &self.mpris {
                        mcz::mpris::notify_seeked(connection, target).await;
                    }
                }
            }
        }
    }

    async fn handle_control(&mut self, request: ControlRequest) -> bool {
        let action = request.request["action"].as_str().unwrap_or("");
        let value = request.request["value"].as_str().unwrap_or("");
        let reply = match action {
            "ping" => json!({"ok":true}),
            "status" => self.status(),
            "modules" => {
                json!({"modules": self.modules.iter().map(|m| json!({"id":m.manifest.id,"name":m.manifest.name})).collect::<Vec<_>>() })
            }
            "play" => {
                self.handle_mpris(PlayerCommand::Play).await;
                json!({"ok":true})
            }
            "pause" => {
                self.handle_mpris(PlayerCommand::Pause).await;
                json!({"ok":true})
            }
            "toggle" => {
                self.handle_mpris(PlayerCommand::PlayPause).await;
                self.status()
            }
            "next" => {
                self.handle_mpris(PlayerCommand::Next).await;
                json!({"ok":true})
            }
            "stop" => {
                self.handle_mpris(PlayerCommand::Stop).await;
                json!({"ok":true})
            }
            "switch" => match plugin::discover() {
                Ok(modules) => {
                    if let Some(index) = modules.iter().position(|m| m.manifest.id == value) {
                        match modules[index].validate().await {
                            Ok(()) => {
                                self.modules = modules;
                                self.selected = index;
                                self.playlist = plugin::selected_playlist(self.module());
                                self.queue.clear();
                                self.clear_current();
                                self.active = true;
                                self.retry_at = Instant::now();
                                json!({"ok":true,"module":value})
                            }
                            Err(error) => json!({"error":error.to_string()}),
                        }
                    } else {
                        json!({"error":format!("Модуль {value} не найден")})
                    }
                }
                Err(error) => json!({"error":error.to_string()}),
            },
            "playlist" => match plugin::save_playlist(&self.module().manifest.id, value) {
                Ok(()) => {
                    self.playlist = value.into();
                    self.queue.clear();
                    self.clear_current();
                    self.active = true;
                    json!({"ok":true})
                }
                Err(error) => json!({"error":error.to_string()}),
            },
            "playlists" => {
                let module = self.module().clone();
                let selected = self.playlist.clone();
                tokio::spawn(async move {
                    let reply = match module.playlists().await {
                        Ok(playlists) => {
                            json!({"playlist":selected,"playlists":playlists.iter().map(|p|json!({"id":p.id,"name":p.name})).collect::<Vec<_>>()})
                        }
                        Err(error) => json!({"error":error.to_string()}),
                    };
                    let _ = request.answer.send(reply);
                });
                return false;
            }
            "settings" | "set-setting" => {
                let module = self.module().clone();
                let is_settings = action == "settings";
                let key = request.request["key"].as_str().unwrap_or("").to_owned();
                let value = value.to_owned();
                tokio::spawn(async move {
                    let result = if is_settings {
                        module.json("settings", &[]).await
                    } else {
                        module.json("set-setting", &[&key, &value]).await
                    };
                    let reply = result.unwrap_or_else(|error| json!({"error":error.to_string()}));
                    let _ = request.answer.send(reply);
                });
                return false;
            }
            "quit" => {
                let _ = request.answer.send(json!({"ok":true}));
                return true;
            }
            _ => json!({"error":format!("Неизвестная команда: {action}")}),
        };
        let _ = request.answer.send(reply);
        false
    }

    async fn set_track(&mut self, track: Track, file: NamedTempFile) -> Result<()> {
        let reader = BufReader::new(file.reopen()?);
        let source = Decoder::new(reader)?;
        self.sink.stop();
        self.sink.append(source);
        self.sink.play();
        *self.title.write().await = track.title.clone();
        *self.artist.write().await = track.artist.clone();
        *self.art_url.write().await = track.art_url.clone();
        *self.track_id.write().await = format!("{}_{}", self.module().manifest.id, track.id);
        *self.duration_us.write().await = track.duration_ms * 1000;
        if let Some(connection) = &self.mpris {
            let meta = build_metadata_map(
                &track.title,
                &track.artist,
                &track.art_url,
                &self.track_id.read().await,
                track.duration_ms * 1000,
                "",
                "/org/mz/Track",
            );
            let mut changed = HashMap::new();
            changed.insert("Metadata", BusValue::from(meta));
            changed.insert("PlaybackStatus", BusValue::from("Playing"));
            notify_changed(connection, changed).await;
        }
        log::info!(
            "▶ {} — {} [{}]",
            track.artist,
            track.title,
            self.module().manifest.id
        );
        self.current = Some(track);
        self.current_file = Some(file);
        Ok(())
    }
}

pub async fn run(modules: Vec<Module>, selected: usize) -> Result<()> {
    let (_stream, output) = rodio::OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&output)?);
    let (mut player, mut mpris_rx) = Player::new(modules, selected, sink).await;
    let (tx, mut rx) = mpsc::channel(32);
    let _server = crate::ipc::listen(tx).await?;
    log::info!("MusicZero: {}", player.module().manifest.name);
    loop {
        if player.current.is_some() && player.sink.empty() {
            player.current = None;
            player.current_file = None;
        }
        if player.active && player.current.is_none() && Instant::now() >= player.retry_at {
            if player.queue.is_empty() {
                let module = player.module().clone();
                let playlist = player.playlist.clone();
                let mut task = tokio::spawn(async move { module.tracks(&playlist).await });
                loop {
                    tokio::select! {
                        result = &mut task => {
                            match result {
                                Ok(Ok(tracks)) if !tracks.is_empty() => player.queue.extend(tracks),
                                Ok(Ok(_)) => player.retry_at = Instant::now() + Duration::from_secs(10),
                                Ok(Err(error)) => { log::error!("Треки: {error}"); player.retry_at = Instant::now() + Duration::from_secs(10); }
                                Err(error) => { log::error!("Задача модуля: {error}"); player.retry_at = Instant::now() + Duration::from_secs(10); }
                            }
                            break;
                        }
                        Some(request) = rx.recv() => {
                            let action = request.request["action"].as_str().unwrap_or("").to_owned();
                            if player.handle_control(request).await { task.abort(); player.sink.stop(); return Ok(()); }
                            if matches!(action.as_str(), "switch" | "playlist" | "pause" | "stop" | "next" | "toggle") && !player.active {
                                task.abort(); break;
                            }
                            if matches!(action.as_str(), "switch" | "playlist" | "next") { task.abort(); break; }
                        }
                        Some(command) = mpris_rx.recv() => {
                            let reset = matches!(command, PlayerCommand::Next | PlayerCommand::Stop | PlayerCommand::Pause | PlayerCommand::PlayPause);
                            player.handle_mpris(command).await;
                            if reset { task.abort(); break; }
                        }
                        _ = mcz::shutdown::wait() => { task.abort(); player.sink.stop(); return Ok(()); },
                    }
                }
                continue;
            }
            let track = player.queue.pop_front().expect("queue checked");
            let module = player.module().clone();
            let id = track.id.clone();
            let mut task = tokio::spawn(async move { module.audio(&id).await });
            loop {
                tokio::select! {
                    result = &mut task => {
                        match result {
                            Ok(Ok(file)) => if let Err(error) = player.set_track(track, file).await {
                                log::warn!("Декодирование: {error}");
                                player.retry_at = Instant::now() + Duration::from_secs(10);
                            },
                            Ok(Err(error)) => {
                                log::warn!("Аудио: {error}");
                                player.retry_at = Instant::now() + Duration::from_secs(10);
                            }
                            Err(error) => {
                                log::warn!("Задача аудио: {error}");
                                player.retry_at = Instant::now() + Duration::from_secs(10);
                            }
                        }
                        break;
                    }
                    Some(request) = rx.recv() => {
                        let action = request.request["action"].as_str().unwrap_or("").to_owned();
                        if player.handle_control(request).await { task.abort(); player.sink.stop(); return Ok(()); }
                        if matches!(action.as_str(), "pause" | "stop" | "toggle") && !player.active {
                            player.queue.push_front(track.clone()); task.abort(); break;
                        }
                        if matches!(action.as_str(), "switch" | "playlist" | "next") { task.abort(); break; }
                    }
                    Some(command) = mpris_rx.recv() => {
                        let reset = matches!(command, PlayerCommand::Next | PlayerCommand::Stop | PlayerCommand::Pause | PlayerCommand::PlayPause);
                        let skip = matches!(command, PlayerCommand::Next);
                        player.handle_mpris(command).await;
                        if reset {
                            if !skip && !player.active { player.queue.push_front(track.clone()); }
                            task.abort(); break;
                        }
                    }
                    _ = mcz::shutdown::wait() => { task.abort(); player.sink.stop(); return Ok(()); },
                }
            }
            continue;
        }
        tokio::select! {
            Some(request) = rx.recv() => if player.handle_control(request).await { break; },
            Some(command) = mpris_rx.recv() => player.handle_mpris(command).await,
            _ = mcz::shutdown::wait() => break,
            _ = tokio::time::sleep(Duration::from_millis(200)) => {},
        }
    }
    player.sink.stop();
    Ok(())
}
