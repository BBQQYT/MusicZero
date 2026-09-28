use crate::mpris::{notify_changed, notify_seeked, PlayerCommand};
use rodio::Sink;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use zbus::zvariant::Value;
use zbus::Connection;

/// The same playback command handling is used by both service adapters.
pub fn spawn_commands(
    mut rx: mpsc::UnboundedReceiver<PlayerCommand>,
    sink: Arc<Sink>,
    skip: Arc<AtomicBool>,
    duration_us: Arc<RwLock<i64>>,
    conn: Connection,
) {
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let status = match cmd {
                PlayerCommand::Play => {
                    sink.play();
                    Some("Playing")
                }
                PlayerCommand::Pause => {
                    sink.pause();
                    Some("Paused")
                }
                PlayerCommand::PlayPause => {
                    if sink.is_paused() {
                        sink.play();
                        Some("Playing")
                    } else {
                        sink.pause();
                        Some("Paused")
                    }
                }
                PlayerCommand::Next => {
                    skip.store(true, Ordering::SeqCst);
                    sink.stop();
                    None
                }
                PlayerCommand::Stop => {
                    sink.stop();
                    Some("Stopped")
                }
                PlayerCommand::Seek(offset) => {
                    let total = *duration_us.read().await;
                    let target = (sink.get_pos().as_micros() as i64 + offset).clamp(0, total);
                    if sink.try_seek(Duration::from_micros(target as u64)).is_ok() {
                        notify_seeked(&conn, target).await;
                    }
                    None
                }
                PlayerCommand::SetPosition(position) => {
                    let total = *duration_us.read().await;
                    let target = position.clamp(0, total);
                    if sink.try_seek(Duration::from_micros(target as u64)).is_ok() {
                        notify_seeked(&conn, target).await;
                    }
                    None
                }
            };
            if let Some(status) = status {
                let mut changed = HashMap::new();
                changed.insert("PlaybackStatus", Value::from(status));
                notify_changed(&conn, changed).await;
            }
        }
    });
}
