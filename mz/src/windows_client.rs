use mcz::windows::{call, Request, Response};
use std::error::Error;

pub const YMZ_SERVICE: &str = "ymz";
pub const YOUMZ_SERVICE: &str = "youmz";

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub service_label: &'static str,
    pub status: String,
    pub artist: String,
    pub title: String,
    pub playlist_id: String,
    pub playlist_name: String,
    pub wave_settings: Option<(String, String, String)>,
}

pub struct DbusClient;

impl DbusClient {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self)
    }

    async fn request(
        &self,
        service: &str,
        action: &str,
        key: &str,
        value: &str,
    ) -> Result<Response, Box<dyn Error>> {
        Ok(call(
            service,
            Request {
                action: action.into(),
                key: key.into(),
                value: value.into(),
            },
        )
        .await?)
    }

    pub async fn is_service_running(&self, service: &str) -> bool {
        self.request(service, "ping", "", "").await.is_ok()
    }

    pub async fn resolve_service(
        &self,
        preferred: Option<&str>,
    ) -> Option<(&'static str, &'static str)> {
        let ymz = self.is_service_running(YMZ_SERVICE).await;
        let youmz = self.is_service_running(YOUMZ_SERVICE).await;
        if let Some(pref) = preferred {
            let pref = pref.to_lowercase();
            if (pref.contains("youmz") || pref.contains("youtube")) && youmz {
                return Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"));
            }
            if (pref.contains("ymz") || pref.contains("yandex")) && ymz {
                return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
            }
        }
        if ymz && self.get_playback_status(YMZ_SERVICE).await.ok().as_deref() == Some("Playing") {
            return Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"));
        }
        if youmz
            && self
                .get_playback_status(YOUMZ_SERVICE)
                .await
                .ok()
                .as_deref()
                == Some("Playing")
        {
            return Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"));
        }
        if ymz {
            Some((YMZ_SERVICE, "Яндекс Музыка (YMZ)"))
        } else if youmz {
            Some((YOUMZ_SERVICE, "YouTube Music (YouMZ)"))
        } else {
            None
        }
    }

    pub async fn get_playback_status(&self, service: &str) -> Result<String, Box<dyn Error>> {
        Ok(self.request(service, "status", "", "").await?.status)
    }

    pub async fn play(&self, service: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "play", "", "").await?;
        Ok(())
    }
    pub async fn pause(&self, service: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "pause", "", "").await?;
        Ok(())
    }
    pub async fn play_pause(&self, service: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "toggle", "", "").await?;
        Ok(())
    }
    pub async fn next(&self, service: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "next", "", "").await?;
        Ok(())
    }
    pub async fn stop(&self, service: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "stop", "", "").await?;
        Ok(())
    }
    pub async fn previous(&self, _service: &str) -> Result<(), Box<dyn Error>> {
        Err("Возврат к предыдущему треку пока не поддерживается".into())
    }

    pub async fn get_status(
        &self,
        service: &'static str,
        label: &'static str,
    ) -> Result<StatusInfo, Box<dyn Error>> {
        let response = self.request(service, "status", "", "").await?;
        let playlist_name = response
            .playlists
            .iter()
            .find(|(id, _)| id == &response.playlist_id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| response.playlist_id.clone());
        Ok(StatusInfo {
            service_label: label,
            status: response.status,
            artist: response.artist,
            title: response.title,
            playlist_id: response.playlist_id,
            playlist_name,
            wave_settings: response.wave,
        })
    }

    pub async fn list_playlists(
        &self,
        service: &str,
    ) -> Result<(String, Vec<(String, String)>), Box<dyn Error>> {
        let response = self.request(service, "playlists", "", "").await?;
        Ok((response.playlist_id, response.playlists))
    }

    pub async fn set_playlist(&self, service: &str, id: &str) -> Result<(), Box<dyn Error>> {
        self.request(service, "set_playlist", "", id).await?;
        Ok(())
    }

    pub async fn get_wave_settings(&self) -> Result<(String, String, String), Box<dyn Error>> {
        self.request(YMZ_SERVICE, "wave", "", "")
            .await?
            .wave
            .ok_or_else(|| "No wave settings".into())
    }

    pub async fn set_wave_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
        self.request(YMZ_SERVICE, "set_wave", key, value).await?;
        Ok(())
    }
}
