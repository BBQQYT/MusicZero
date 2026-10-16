use crate::Result;
use serde::{Deserialize, Serialize};

pub const APP: &str = "mz-spotify";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub client_id: String,
    pub redirect_uri: String,
    pub bitrate: u32,
    pub explicit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            redirect_uri: "http://127.0.0.1:8910/callback".into(),
            bitrate: 320,
            explicit: true,
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self> {
        mz_module_support::load(APP)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.client_id.is_empty()
            && (self.client_id.len() != 32
                || !self.client_id.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Spotify client_id must be 32 hexadecimal characters".into());
        }
        callback_address(&self.redirect_uri)?;
        if ![96, 160, 320].contains(&self.bitrate) {
            return Err("Spotify bitrate must be 96, 160 or 320".into());
        }
        Ok(())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let mut next = self.clone();
        match key {
            "client_id" => next.client_id = value.to_owned(),
            "redirect_uri" => next.redirect_uri = value.to_owned(),
            "bitrate" => next.bitrate = value.parse()?,
            "explicit" => next.explicit = mz_module_support::boolean(value)?,
            _ => return Err(format!("Unknown Spotify setting: {key}").into()),
        }
        next.validate()?;
        mz_module_support::save_settings(APP, &next)?;
        *self = next;
        Ok(())
    }
}

pub fn callback_address(uri: &str) -> Result<std::net::SocketAddr> {
    let url = url::Url::parse(uri)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/callback"
    {
        return Err("Spotify redirect_uri must be http://127.0.0.1:<port>/callback".into());
    }
    let port = url
        .port()
        .ok_or("Spotify callback needs an explicit port")?;
    if port == 0 {
        return Err("Spotify callback port cannot be zero".into());
    }
    Ok(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callbacks_cannot_bind_public_interfaces_or_redirect_elsewhere() {
        assert!(Settings::default().validate().is_ok());
        for uri in [
            "http://0.0.0.0:8910/callback",
            "http://localhost:8910/callback",
            "http://127.0.0.1:8910/callback?x=1",
            "http://user@127.0.0.1:8910/callback",
            "http://127.0.0.1:0/callback",
        ] {
            assert!(callback_address(uri).is_err(), "{uri}");
        }
    }

    #[test]
    fn settings_reject_broken_client_ids_bitrates_uri_without_saving() {
        let mut settings = Settings::default();
        for (key, value) in [
            ("client_id", "zzz"),
            ("client_id", "abc"),
            ("bitrate", "128"),
            ("bitrate", "x"),
            ("explicit", "maybe"),
            ("redirect_uri", "https://example.org/callback"),
        ] {
            assert!(settings.set(key, value).is_err(), "{key}={value}");
        }
    }
}
