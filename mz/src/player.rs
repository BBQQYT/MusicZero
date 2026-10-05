use crate::ipc::ControlRequest;
use crate::plugin::{self, Module, Track};
use crate::seek::SeekRequest;
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
    generation: u64,
    retry_at: Instant,
    mpris: Option<zbus::Connection>,
    title: Arc<RwLock<String>>,
    artist: Arc<RwLock<String>>,
    art_url: Arc<RwLock<String>>,
    track_id: Arc<RwLock<String>>,
    duration_us: Arc<RwLock<i64>>,
    seekable: Arc<RwLock<bool>>,
    feedback: crate::feedback::Worker,
    continuous: bool,
    after: Option<String>,
    recent: VecDeque<String>,
    played: Duration,
    playing_since: Option<Instant>,
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
        let seekable = Arc::new(RwLock::new(false));
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
                current_can_seek: seekable.clone(),
                track_url_prefix: "",
                track_path_prefix: "/org/mz/Track",
            };
            match async {
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
                    .await
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
                generation: 0,
                retry_at: Instant::now(),
                mpris,
                title,
                artist,
                art_url,
                track_id,
                duration_us,
                seekable,
                feedback: crate::feedback::Worker::new(),
                continuous: false,
                after: None,
                recent: VecDeque::new(),
                played: Duration::ZERO,
                playing_since: None,
            },
            cmd_rx,
        )
    }

    fn module(&self) -> &Module {
        &self.modules[self.selected]
    }

    async fn status(&self) -> Value {
        let status = if !self.active {
            "Paused"
        } else if self.current.is_some() && !self.sink.empty() {
            "Playing"
        } else {
            "Loading"
        };
        json!({"module": self.module().manifest.id, "name": self.module().manifest.name,
            "playlist": self.playlist, "status": status,
            "track_id": self.current.as_ref().map(|t| t.id.as_str()).unwrap_or(""),
            "title": self.current.as_ref().map(|t| t.title.as_str()).unwrap_or(""),
            "artist": self.current.as_ref().map(|t| t.artist.as_str()).unwrap_or(""),
            "position_ms": self.sink.get_pos().as_millis().min(u64::MAX as u128) as u64,
            "duration_ms": if self.current.is_some() { (*self.duration_us.read().await).max(0) / 1000 } else { 0 },
            "can_seek": self.current.as_ref().is_some_and(|track| !track.stream) && !self.sink.empty()})
    }

    fn set_active(&mut self, active: bool) {
        if let Some(since) = self.playing_since.take() {
            self.played = self.played.saturating_add(since.elapsed());
        }
        self.active = active;
        if active && self.current.is_some() {
            self.playing_since = Some(Instant::now());
        }
    }

    fn reset_continuation(&mut self) {
        self.continuous = false;
        self.after = None;
        self.recent.clear();
    }

    fn accept_tracks(&mut self, mut batch: plugin::TrackList) {
        self.continuous = batch.continuous;
        if batch.continuous {
            let mut seen: std::collections::HashSet<String> = self.recent.iter().cloned().collect();
            seen.extend(self.queue.iter().map(|track| track.id.clone()));
            if let Some(track) = &self.current {
                seen.insert(track.id.clone());
            }
            batch.tracks.retain(|track| seen.insert(track.id.clone()));
        }
        self.queue.extend(batch.tracks);
    }

    async fn clear_current(&mut self, event: &'static str) {
        if let Some(track) = &self.current {
            let played = self.played.saturating_add(
                self.playing_since
                    .map(|since| since.elapsed())
                    .unwrap_or_default(),
            );
            self.feedback
                .sender
                .emit(self.module().clone(), track, event, played);
            if self.continuous {
                self.after = Some(track.id.clone());
                self.recent.push_back(track.id.clone());
                if self.recent.len() > 256 {
                    self.recent.pop_front();
                }
            }
        }
        self.played = Duration::ZERO;
        self.playing_since = None;
        self.sink.stop();
        self.current = None;
        self.current_file = None;
        *self.seekable.write().await = false;
        *self.duration_us.write().await = 0;
        self.title.write().await.clear();
        self.artist.write().await.clear();
        self.art_url.write().await.clear();
        self.track_id.write().await.clear();
        if let Some(connection) = &self.mpris {
            let mut changed = HashMap::new();
            changed.insert("CanSeek", BusValue::from(false));
            changed.insert(
                "Metadata",
                BusValue::from(build_metadata_map("", "", "", "", 0, "", "/org/mz/Track")),
            );
            notify_changed(connection, changed).await;
        }
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
                self.set_active(true);
                self.sink.play();
                self.notify_status("Playing").await;
            }
            PlayerCommand::Pause => {
                self.set_active(false);
                self.sink.pause();
                self.notify_status("Paused").await;
            }
            PlayerCommand::PlayPause => {
                self.set_active(!self.active);
                if self.active {
                    self.sink.play();
                    self.notify_status("Playing").await;
                } else {
                    self.sink.pause();
                    self.notify_status("Paused").await;
                }
            }
            PlayerCommand::Next => {
                self.generation = self.generation.wrapping_add(1);
                self.retry_at = Instant::now();
                self.active = true;
                self.clear_current("skip").await;
            }
            PlayerCommand::Stop => {
                self.active = false;
                self.clear_current("skip").await;
                self.notify_status("Stopped").await;
            }
            PlayerCommand::Seek(offset) => {
                if let Err(error) = self.seek(SeekRequest::Relative(offset)).await {
                    log::debug!("MPRIS seek: {error}");
                }
            }
            PlayerCommand::SetPosition(position) => {
                if position >= 0 {
                    if let Err(error) = self.seek(SeekRequest::Absolute(position)).await {
                        log::debug!("MPRIS set position: {error}");
                    }
                }
            }
        }
    }

    async fn seek(&mut self, request: SeekRequest) -> Result<u64> {
        let track = self
            .current
            .as_ref()
            .ok_or("Нет текущего трека для перемотки")?;
        if track.stream {
            return Err("Перемотка недоступна для прямого эфира".into());
        }
        if self.sink.empty() {
            return Err("Трек уже завершён".into());
        }
        let position = self.sink.get_pos().as_micros().min(i64::MAX as u128) as i64;
        let target = request.target(position, *self.duration_us.read().await);
        self.sink
            .try_seek(Duration::from_micros(target))
            .map_err(|error| format!("Не удалось перемотать трек: {error}"))?;
        let actual = self.sink.get_pos().as_micros().min(u64::MAX as u128) as u64;
        if let Some(connection) = &self.mpris {
            mcz::mpris::notify_seeked(connection, actual.min(i64::MAX as u64) as i64).await;
        }
        Ok(actual)
    }

    async fn handle_control(&mut self, request: ControlRequest) -> bool {
        let action = request.request["action"].as_str().unwrap_or("");
        let value = request.request["value"].as_str().unwrap_or("");
        if matches!(action, "set-setting" | "playlist")
            && request.request["module"]
                .as_str()
                .is_some_and(|id| id != self.module().manifest.id)
        {
            let _ = request
                .answer
                .send(json!({"error":"Active module changed; retry the setting"}));
            return false;
        }
        let reply = match action {
            "ping" => json!({"ok":true}),
            "status" => self.status().await,
            "seek" => {
                let result = match SeekRequest::from_wire(
                    value,
                    request.request["key"].as_str().unwrap_or(""),
                ) {
                    Ok(seek) => self.seek(seek).await,
                    Err(error) => Err(error),
                };
                match result {
                    Ok(position) => json!({"ok":true,"position_ms":position / 1000}),
                    Err(error) => json!({"error":error.to_string()}),
                }
            }
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
                self.status().await
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
                                self.generation = self.generation.wrapping_add(1);
                                self.clear_current("skip").await;
                                self.reset_continuation();
                                self.modules = modules;
                                self.selected = index;
                                self.playlist = plugin::selected_playlist(self.module());
                                self.queue.clear();
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
                    self.generation = self.generation.wrapping_add(1);
                    self.retry_at = Instant::now();
                    self.playlist = value.trim().into();
                    self.queue.clear();
                    self.clear_current("skip").await;
                    self.reset_continuation();
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
            "set-setting" if matches!(self.module().manifest.id.as_str(), "local" | "icecast") => {
                let key = request.request["key"].as_str().unwrap_or("");
                match self
                    .module()
                    .config_json("set-setting", &[key, value])
                    .await
                {
                    Ok(reply) => {
                        self.generation = self.generation.wrapping_add(1);
                        self.queue.clear();
                        self.clear_current("skip").await;
                        self.reset_continuation();
                        self.retry_at = Instant::now();
                        reply
                    }
                    Err(error) => json!({"error":error.to_string()}),
                }
            }
            "settings" | "set-setting" => {
                let module = self.module().clone();
                let is_settings = action == "settings";
                let key = request.request["key"].as_str().unwrap_or("").to_owned();
                let value = value.to_owned();
                tokio::spawn(async move {
                    let result = if is_settings {
                        module.config_json("settings", &[]).await
                    } else {
                        module.config_json("set-setting", &[&key, &value]).await
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
        let source = decode_audio(&file)?;
        self.begin_track(track, source, Some(file)).await
    }

    async fn begin_track<S: rodio::Source + Send + 'static>(
        &mut self,
        mut track: Track,
        source: S,
        file: Option<NamedTempFile>,
    ) -> Result<()> {
        if track.stream {
            track.duration_ms = 0;
        } else if let Some(duration) = source.total_duration() {
            track.duration_ms = duration.as_millis().min((i64::MAX / 1000) as u128) as i64;
        }
        self.sink.stop();
        self.sink.append(source);
        self.sink.play();
        *self.title.write().await = track.title.clone();
        *self.artist.write().await = track.artist.clone();
        *self.art_url.write().await = track.art_url.clone();
        *self.track_id.write().await = format!("{}_{}", self.module().manifest.id, track.id);
        let clamped_ms = track.duration_ms.clamp(0, i64::MAX / 1000);
        *self.duration_us.write().await = clamped_ms * 1000;
        *self.seekable.write().await = !track.stream;
        if let Some(connection) = &self.mpris {
            let meta = build_metadata_map(
                &track.title,
                &track.artist,
                &track.art_url,
                &self.track_id.read().await,
                clamped_ms * 1000,
                "",
                "/org/mz/Track",
            );
            let mut changed = HashMap::new();
            changed.insert("Metadata", BusValue::from(meta));
            changed.insert("CanSeek", BusValue::from(!track.stream));
            changed.insert("PlaybackStatus", BusValue::from("Playing"));
            notify_changed(connection, changed).await;
        }
        log::info!(
            "▶ {} — {} [{}]",
            track.artist,
            track.title,
            self.module().manifest.id
        );
        self.feedback.sender.emit(
            self.module().clone(),
            &track,
            "trackStarted",
            Duration::ZERO,
        );
        self.played = Duration::ZERO;
        self.playing_since = Some(Instant::now());
        self.current = Some(track);
        self.current_file = file;
        Ok(())
    }
}

enum Audio {
    File(NamedTempFile),
    Live(crate::live::LiveSource),
}

enum Loaded {
    Tracks(Result<plugin::TrackList>),
    Audio(Track, Result<Audio>),
}

type LoadFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Loaded> + Send>>;

struct Preload {
    pending: Option<LoadFuture>,
    loading_track: Option<Track>,
    ready: Option<(Track, Audio)>,
}

impl Preload {
    fn new() -> Self {
        Self {
            pending: None,
            loading_track: None,
            ready: None,
        }
    }

    fn start(
        &mut self,
        module: Module,
        playlist: String,
        queue: &mut VecDeque<Track>,
        after: Option<String>,
        feedback: crate::feedback::Sender,
    ) {
        if self.pending.is_some() || self.ready.is_some() {
            return;
        }
        if let Some(track) = queue.pop_front() {
            self.loading_track = Some(track.clone());
            self.pending = Some(Box::pin(async move {
                let result = if track.stream {
                    crate::live::LiveSource::open(module, track.id.clone(), track.buffer_ms)
                        .await
                        .map(Audio::Live)
                } else {
                    module.audio(&track.id).await.map(Audio::File)
                };
                Loaded::Audio(track, result)
            }));
        } else {
            self.pending = Some(Box::pin(async move {
                if after.is_some() {
                    if let Err(error) = feedback.drain().await {
                        return Loaded::Tracks(Err(error));
                    }
                }
                Loaded::Tracks(module.tracks(&playlist, after.as_deref()).await)
            }));
        }
    }

    fn clear(&mut self) {
        // Dropping the future also drops the module child (kill_on_drop) and temp file.
        self.pending = None;
        self.loading_track = None;
        self.ready = None;
    }

    fn stop(&mut self, queue: &mut VecDeque<Track>) {
        self.pending = None;
        if let Some(track) = self.loading_track.take() {
            queue.push_front(track);
        }
    }
}

pub async fn run(modules: Vec<Module>, selected: usize) -> Result<()> {
    let stream = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = Arc::new(Sink::connect_new(stream.mixer()));
    let (mut player, mut mpris_rx) = Player::new(modules, selected, sink).await;
    let (tx, mut rx) = mpsc::channel(32);
    let _server = crate::ipc::listen(tx).await?;
    log::info!("MusicZero: {}", player.module().manifest.name);
    let mut preload = Preload::new();
    let mut consecutive_failures: u32 = 0;
    let backoff =
        |failures: u32| Duration::from_secs((2u64.saturating_pow(failures.min(6)) * 5).min(300));
    let shutdown = mcz::shutdown::wait();
    tokio::pin!(shutdown);
    loop {
        if player.current.is_some() && player.sink.empty() {
            player.clear_current("trackFinished").await;
            player.retry_at = Instant::now();
        }
        if player.active && player.current.is_none() {
            if let Some((track, file)) = preload.ready.take() {
                let result = match file {
                    Audio::File(file) => player.set_track(track, file).await,
                    Audio::Live(source) => player.begin_track(track, source, None).await,
                };
                if let Err(error) = result {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    log::warn!("Декодирование: {error}");
                    player.retry_at = Instant::now() + backoff(consecutive_failures);
                } else {
                    consecutive_failures = 0;
                }
            }
        }
        // Keep only the current file plus one upcoming audio file on disk.
        // Refill metadata too when the current track is the last in the batch.
        if player.active
            && player.current.as_ref().is_none_or(|track| !track.stream)
            && preload.pending.is_none()
            && preload.ready.is_none()
            && Instant::now() >= player.retry_at
        {
            while player.queue.front().is_some_and(|track| {
                track.id.is_empty()
                    || track.id.len() > 512
                    || track.id.chars().any(char::is_control)
            }) {
                log::warn!("Пропуск трека с некорректным id");
                player.queue.pop_front();
            }
            preload.start(
                player.module().clone(),
                player.playlist.clone(),
                &mut player.queue,
                player.after.clone(),
                player.feedback.sender.clone(),
            );
        }
        tokio::select! {
            result = async { preload.pending.as_mut().expect("guarded pending load").as_mut().await }, if preload.pending.is_some() => {
                preload.pending = None;
                preload.loading_track = None;
                match result {
                    Loaded::Tracks(Ok(batch)) if !batch.tracks.is_empty() => {
                        player.accept_tracks(batch);
                        if player.queue.is_empty() {
                            player.retry_at = Instant::now() + Duration::from_secs(5);
                        }
                    }
                    Loaded::Audio(track, Ok(file)) => {
                        preload.ready = Some((track, file));
                        // Start immediately if playback ended while downloading.
                    }
                    result => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        match result {
                            Loaded::Tracks(Ok(_)) => log::warn!("Треки: пустой ответ"),
                            Loaded::Tracks(Err(error)) => log::error!("Треки: {error}"),
                            Loaded::Audio(_, Err(error)) => log::warn!("Аудио: {error}"),
                            _ => unreachable!(),
                        }
                        player.retry_at = Instant::now() + backoff(consecutive_failures);
                    }
                }
            }
            Some(request) = rx.recv() => {
                let action = request.request["action"].as_str().unwrap_or("").to_owned();
                let generation = player.generation;
                let had_current = player.current.is_some();
                if player.handle_control(request).await { break; }
                if player.generation != generation {
                    // Next during playback consumes the already loading/ready track.
                    // Next during loading skips it; successful switch/playlist discard it.
                    if action != "next" || !had_current { preload.clear(); }
                    consecutive_failures = 0;
                }
                if action == "stop" { preload.stop(&mut player.queue); }
            }
            Some(command) = mpris_rx.recv() => {
                let generation = player.generation;
                let had_current = player.current.is_some();
                let stop = matches!(command, PlayerCommand::Stop);
                player.handle_mpris(command).await;
                if player.generation != generation {
                    if !had_current { preload.clear(); }
                    consecutive_failures = 0;
                }
                if stop { preload.stop(&mut player.queue); }
            }
            _ = &mut shutdown => break,
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
        }
    }
    preload.clear();
    player.clear_current("skip").await;
    let _ = tokio::time::timeout(Duration::from_secs(2), player.feedback.sender.drain()).await;
    Ok(())
}

fn decode_audio(file: &NamedTempFile) -> Result<Decoder<BufReader<std::fs::File>>> {
    let len = file.as_file().metadata()?.len();
    if len == 0 || len > 512 * 1024 * 1024 {
        return Err(format!("Некорректный размер аудио: {len}").into());
    }
    Ok(Decoder::builder()
        .with_data(BufReader::new(file.reopen()?))
        .with_byte_len(len)
        .with_seekable(true)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: String::new(),
            art_url: String::new(),
            duration_ms: 120,
            stream: false,
            buffer_ms: 1000,
            feedback: String::new(),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn next_audio_loads_while_current_audio_is_queued() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let audio = include_bytes!("../tests/fixtures/tone.m4a");
        std::fs::write(dir.path().join("tone.m4a"), audio).unwrap();
        let executable = dir.path().join("provider");
        std::fs::write(
            &executable,
            b"#!/bin/sh\ncat \"$(dirname \"$0\")/tone.m4a\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let module = Module {
            executable,
            manifest: plugin::Manifest {
                protocol: 1,
                id: "demo".into(),
                name: "Demo".into(),
                binary: "provider".into(),
                default_playlist: "main".into(),
            },
        };
        // Keep the mixer unconsumed so the first track remains queued without an audio device.
        let (mixer, _output) = rodio::mixer::mixer(2, 44100);
        let sink = Sink::connect_new(&mixer);
        let mut first = NamedTempFile::new().unwrap();
        first.write_all(audio).unwrap();
        sink.append(decode_audio(&first).unwrap());
        let mut queue = VecDeque::from([track("second"), track("third")]);
        let mut preload = Preload::new();
        let feedback = crate::feedback::Worker::new();
        preload.start(
            module.clone(),
            "main".into(),
            &mut queue,
            None,
            feedback.sender.clone(),
        );
        // Starting again must not remove another queued track or start another process.
        preload.start(
            module,
            "main".into(),
            &mut queue,
            None,
            feedback.sender.clone(),
        );
        assert_eq!(queue.front().unwrap().id, "third");
        let loaded = tokio::time::timeout(Duration::from_secs(5), preload.pending.take().unwrap())
            .await
            .unwrap();
        let Loaded::Audio(track, file) = loaded else {
            panic!("expected next audio")
        };
        let Audio::File(file) = file.unwrap() else {
            panic!("expected file")
        };
        assert_eq!(track.id, "second");
        assert!(decode_audio(&file).unwrap().count() > 0);
        assert!(
            !sink.empty(),
            "current audio must not be stopped by preloading"
        );
        let path = file.path().to_path_buf();
        preload.ready = Some((track, Audio::File(file)));
        preload.clear();
        assert!(
            !path.exists(),
            "changing source must delete stale preloaded audio"
        );
    }

    #[test]
    fn stop_restores_the_track_being_downloaded() {
        let mut preload = Preload::new();
        preload.loading_track = Some(track("second"));
        preload.pending = Some(Box::pin(std::future::pending()));
        let mut queue = VecDeque::from([track("third")]);
        preload.stop(&mut queue);
        assert!(preload.pending.is_none());
        assert_eq!(queue.pop_front().unwrap().id, "second");
        assert_eq!(queue.pop_front().unwrap().id, "third");
    }

    #[test]
    fn invalid_audio_returns_an_error() {
        let mut file = NamedTempFile::new().unwrap();
        assert!(decode_audio(&file).is_err());
        file.write_all(b"not an audio file").unwrap();
        assert!(decode_audio(&file).is_err());
    }

    #[test]
    fn m4a_file_decodes_to_audio_samples() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(include_bytes!("../tests/fixtures/tone.m4a"))
            .unwrap();
        let decoder = decode_audio(&file).unwrap();
        assert!(decoder.count() > 0);
    }
}
