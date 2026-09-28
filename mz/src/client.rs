use zbus::names::BusName;
use zbus::Connection;

pub const YMZ_SERVICE: &str = "org.mpris.MediaPlayer2.ymz";
pub const YOUMZ_SERVICE: &str = "org.mpris.MediaPlayer2.youmz";

#[derive(Debug, Clone)]
pub struct StatusInfo {
    #[allow(dead_code)]
    pub service_name: &'static str,
    pub service_label: &'static str,
    pub status: String,
    pub artist: String,
    pub title: String,
    pub playlist_id: String,
    pub playlist_name: String,
    pub wave_settings: Option<(String, String, String)>,
}

pub struct DbusClient {
    conn: Connection,
}

impl DbusClient {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::session().await?;
        Ok(Self { conn })
    }

    /// Проверяет, запущен ли сервис на сессионной шине
    pub async fn is_service_running(&self, service: &str) -> bool {
        let dbus = match zbus::fdo::DBusProxy::new(&self.conn).await {
            Ok(d) => d,
            Err(_) => return false,
        };
        let bus_name = match BusName::try_from(service) {
            Ok(n) => n,
            Err(_) => return false,
        };
        dbus.name_has_owner(bus_name).await.unwrap_or(false)
    }

    /// Определяет целевой сервис: если задано предпочтение — использует его,
    /// иначе выбирает играющий плеер, либо первый найденный активный.
    pub async fn resolve_service(&self, preferred: Option<&str>) -> Option<(&'static str, &'static str)> {
        let ymz_running = self.is_service_running(YMZ_SERVICE).await;
        let youmz_running = self.is_service_running(YOUMZ_SERVICE).await;

        if let Some(pref) = preferred {
            let pref_lower = pref.to_lowercase();
            if pref_lower.contains("ymz") || pref_lower.contains("yandex") {
                if ymz_running {
                    return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
                }
            } else if pref_lower.contains("youmz") || pref_lower.contains("youtube") {
                if youmz_running {
                    return Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"));
                }
            }
        }

        // Если запущены оба — отдаём приоритет тому, кто сейчас играет
        if ymz_running && youmz_running {
            if let Ok(st) = self.get_playback_status(YMZ_SERVICE).await {
                if st == "Playing" {
                    return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
                }
            }
            if let Ok(st) = self.get_playback_status(YOUMZ_SERVICE).await {
                if st == "Playing" {
                    return Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"));
                }
            }
            return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
        }

        if ymz_running {
            return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
        }
        if youmz_running {
            return Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"));
        }

        None
    }

    pub async fn get_playback_status(&self, service: &str) -> Result<String, zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        proxy.get_property("PlaybackStatus").await
    }

    pub async fn play(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("Play", &()).await?;
        Ok(())
    }

    pub async fn pause(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("Pause", &()).await?;
        Ok(())
    }

    pub async fn play_pause(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("PlayPause", &()).await?;
        Ok(())
    }

    pub async fn next(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("Next", &()).await?;
        Ok(())
    }

    pub async fn previous(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("Previous", &()).await?;
        Ok(())
    }

    pub async fn stop(&self, service: &str) -> Result<(), zbus::Error> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        let () = proxy.call("Stop", &()).await?;
        Ok(())
    }

    pub async fn get_status(
        &self,
        service: &'static str,
        label: &'static str,
    ) -> Result<StatusInfo, Box<dyn std::error::Error>> {
        let status = self
            .get_playback_status(service)
            .await
            .unwrap_or_else(|_| "Stopped".into());

        let control_proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mcz/Control",
            "org.mcz.Control",
        )
        .await?;

        let (artist, title): (String, String) = control_proxy.call("NowPlaying", &()).await.unwrap_or_default();
        let playlist_id: String = control_proxy.call("CurrentPlaylist", &()).await.unwrap_or_default();
        let playlists: Vec<(String, String)> = control_proxy.call("ListPlaylists", &()).await.unwrap_or_default();

        let playlist_name = playlists
            .iter()
            .find(|(id, _)| id == &playlist_id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| {
                if playlist_id.is_empty() {
                    "По умолчанию".into()
                } else {
                    playlist_id.clone()
                }
            });

        let wave_settings = if service == YMZ_SERVICE {
            control_proxy.call("WaveSettings", &()).await.ok()
        } else {
            None
        };

        Ok(StatusInfo {
            service_name: service,
            service_label: label,
            status,
            artist,
            title,
            playlist_id,
            playlist_name,
            wave_settings,
        })
    }

    pub async fn list_playlists(
        &self,
        service: &str,
    ) -> Result<(String, Vec<(String, String)>), Box<dyn std::error::Error>> {
        let control_proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mcz/Control",
            "org.mcz.Control",
        )
        .await?;

        let current: String = control_proxy.call("CurrentPlaylist", &()).await?;
        let list: Vec<(String, String)> = control_proxy.call("ListPlaylists", &()).await?;
        Ok((current, list))
    }

    pub async fn set_playlist(
        &self,
        service: &str,
        playlist_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let control_proxy = zbus::Proxy::new(
            &self.conn,
            service,
            "/org/mcz/Control",
            "org.mcz.Control",
        )
        .await?;

        let () = control_proxy.call("SetPlaylist", &(playlist_id,)).await?;
        Ok(())
    }

    pub async fn get_wave_settings(&self) -> Result<(String, String, String), Box<dyn std::error::Error>> {
        let control_proxy = zbus::Proxy::new(
            &self.conn,
            YMZ_SERVICE,
            "/org/mcz/Control",
            "org.mcz.Control",
        )
        .await?;

        let wave: (String, String, String) = control_proxy.call("WaveSettings", &()).await?;
        Ok(wave)
    }

    pub async fn set_wave_setting(&self, key: &str, val: &str) -> Result<(), Box<dyn std::error::Error>> {
        let control_proxy = zbus::Proxy::new(
            &self.conn,
            YMZ_SERVICE,
            "/org/mcz/Control",
            "org.mcz.Control",
        )
        .await?;

        let () = control_proxy.call("SetWaveSetting", &(key, val)).await?;
        Ok(())
    }
}
