use rodio::Sink;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use zbus::zvariant::{ObjectPath, Value};
use zbus::{interface, Connection};

#[derive(Debug)]
pub enum PlayerCommand {
    PlayPause,
    Play,
    Pause,
    Next,
    Stop,
    Seek(i64),        // относительное смещение в микросекундах
    SetPosition(i64), // абсолютная позиция в микросекундах
}

pub struct MprisRoot {
    pub identity: &'static str,
    pub mime_types: &'static [&'static str],
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        self.identity
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        self.mime_types.iter().map(|s| s.to_string()).collect()
    }
}

pub struct MprisPlayer {
    pub cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    pub sink: Arc<Sink>,
    pub current_title: Arc<RwLock<String>>,
    pub current_artist: Arc<RwLock<String>>,
    pub current_art_url: Arc<RwLock<String>>,
    pub current_track_id: Arc<RwLock<String>>,
    pub current_duration_us: Arc<RwLock<i64>>,
    pub track_url_prefix: &'static str,
    pub track_path_prefix: &'static str,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    async fn next(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Next);
    }

    async fn play_pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayPause);
    }

    async fn pause(&self) {
        if !self.sink.is_paused() {
            let _ = self.cmd_tx.send(PlayerCommand::Pause);
        }
    }

    async fn play(&self) {
        if self.sink.is_paused() {
            let _ = self.cmd_tx.send(PlayerCommand::Play);
        }
    }

    async fn stop(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
    }

    // Перемотка на offset микросекунд относительно текущей позиции
    async fn seek(&self, offset: i64) {
        let _ = self.cmd_tx.send(PlayerCommand::Seek(offset));
    }

    // Установка ползунка в конкретную точку
    async fn set_position(&self, _track_id: ObjectPath<'_>, position: i64) {
        let _ = self.cmd_tx.send(PlayerCommand::SetPosition(position));
    }

    #[zbus(property)]
    fn playback_status(&self) -> &str {
        if self.sink.is_paused() {
            "Paused"
        } else if self.sink.empty() {
            "Stopped"
        } else {
            "Playing"
        }
    }

    // Текущая позиция в микросекундах для виджета
    #[zbus(property)]
    fn position(&self) -> i64 {
        self.sink.get_pos().as_micros() as i64
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn metadata(&self) -> HashMap<String, Value<'static>> {
        let title = self.current_title.read().await;
        let artist = self.current_artist.read().await;
        let art = self.current_art_url.read().await;
        let tid = self.current_track_id.read().await;
        let duration = *self.current_duration_us.read().await;

        build_metadata_map(
            &title,
            &artist,
            &art,
            &tid,
            duration,
            self.track_url_prefix,
            self.track_path_prefix,
        )
    }
}

pub fn build_metadata_map(
    title: &str,
    artist: &str,
    art_url: &str,
    track_id: &str,
    duration_us: i64,
    url_prefix: &str,
    path_prefix: &str,
) -> HashMap<String, Value<'static>> {
    let mut m = HashMap::new();
    let sanitized_id: String = track_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let path_str = if sanitized_id.is_empty() {
        "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string()
    } else {
        format!("{path_prefix}/{sanitized_id}")
    };

    let track_path = ObjectPath::try_from(path_str).unwrap_or_else(|_| {
        ObjectPath::from_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack")
    });

    m.insert("mpris:trackid".to_string(), Value::from(track_path));
    m.insert("xesam:title".to_string(), Value::from(title.to_string()));
    m.insert(
        "xesam:artist".to_string(),
        Value::from(vec![artist.to_string()]),
    );

    if !track_id.is_empty() && !url_prefix.is_empty() {
        m.insert(
            "xesam:url".to_string(),
            Value::from(format!("{url_prefix}{track_id}")),
        );
    }

    if duration_us > 0 {
        m.insert("mpris:length".to_string(), Value::from(duration_us));
    }

    if !art_url.is_empty() {
        m.insert("mpris:artUrl".to_string(), Value::from(art_url.to_string()));
    }
    m
}

pub async fn notify_changed(conn: &Connection, changed: HashMap<&str, Value<'_>>) {
    let invalidated: Vec<&str> = vec![];
    let _ = conn
        .emit_signal(
            Option::<&str>::None,
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &("org.mpris.MediaPlayer2.Player", changed, invalidated),
        )
        .await;
}

pub async fn notify_seeked(conn: &Connection, position_us: i64) {
    let _ = conn
        .emit_signal(
            Option::<&str>::None,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
            "Seeked",
            &(position_us,),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata_has_distinct_urls_and_valid_track_paths() {
        for (url, path) in [
            ("https://music.yandex.ru/album/0/track/", "/org/ymz/Track"),
            ("https://www.youtube.com/watch?v=", "/org/youmz/Track"),
        ] {
            let metadata = build_metadata_map("Song", "Artist", "", "abc:123", 42, url, path);
            assert!(metadata.contains_key("xesam:url"));
            assert!(metadata.contains_key("mpris:trackid"));
            assert!(metadata.contains_key("mpris:length"));
        }
    }
}
