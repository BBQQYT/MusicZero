use crate::{
    config::{Settings, APP},
    Result,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const SCOPES: &str = "user-read-private user-read-email user-library-read user-library-modify playlist-read-private playlist-read-collaborative playlist-modify-private playlist-modify-public user-follow-read user-follow-modify user-top-read user-read-recently-played user-read-playback-state user-modify-playback-state user-read-currently-playing streaming";

// Intentionally not Debug: credentials must never appear in diagnostics.
#[derive(Clone, Deserialize, Serialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub scope: String,
    pub client_id: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    scope: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn token_path() -> std::path::PathBuf {
    mz_module_support::paths::config_dir(APP).join("tokens.json")
}

impl Tokens {
    pub fn load() -> Result<Self> {
        mz_module_support::read_json(&token_path())
    }
    pub fn save(&self) -> Result<()> {
        mz_module_support::save(&token_path(), self)
    }

    pub async fn fresh(&mut self, client: &reqwest::Client, settings: &Settings) -> Result<String> {
        if self.client_id != settings.client_id {
            return Err("Spotify client_id changed; sign in again".into());
        }
        if self.expires_at <= now().saturating_add(60) {
            let response = exchange(
                client,
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &self.refresh_token),
                    ("client_id", &settings.client_id),
                ],
            )
            .await?;
            self.access_token = response.access_token;
            if let Some(refresh) = response.refresh_token {
                self.refresh_token = refresh;
            }
            self.expires_at = now().saturating_add(response.expires_in);
            if !response.scope.is_empty() {
                self.scope = response.scope;
            }
            self.save()?;
        }
        Ok(self.access_token.clone())
    }
}

async fn exchange(client: &reqwest::Client, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let response = client
        .post("https://accounts.spotify.com/api/token")
        .form(form)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!(
            "Spotify token exchange failed (HTTP {}); sign in again or check the app configuration",
            response.status()
        )
        .into());
    }
    Ok(response.json().await?)
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn callback_code(target: &str, state: &str) -> Result<String> {
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))?;
    if url.path() != "/callback" {
        return Err("Not an OAuth callback".into());
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    let states = pairs
        .iter()
        .filter(|(k, _)| k == "state")
        .collect::<Vec<_>>();
    if states.len() != 1 || states[0].1 != state {
        return Err("Spotify OAuth state mismatch".into());
    }
    if pairs.iter().any(|(k, _)| k == "error") {
        return Err("Spotify authorization was denied".into());
    }
    let codes = pairs
        .iter()
        .filter(|(k, _)| k == "code")
        .collect::<Vec<_>>();
    if codes.len() != 1 || codes[0].1.is_empty() {
        return Err("Missing Spotify authorization code".into());
    }
    Ok(codes[0].1.to_string())
}

pub async fn login(settings: &Settings, client: &reqwest::Client) -> Result<()> {
    settings.validate()?;
    if settings.client_id.is_empty() {
        return Err(
            "Set your Spotify Developer app client_id first: mz set spotify client_id <id>".into(),
        );
    }
    let listener =
        tokio::net::TcpListener::bind(crate::config::callback_address(&settings.redirect_uri)?)
            .await?;
    let verifier = random_secret();
    let state = random_secret();
    let challenge = challenge(&verifier);
    let mut url = url::Url::parse("https://accounts.spotify.com/authorize")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", settings.client_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", &settings.redirect_uri),
        ("scope", SCOPES),
        ("state", &state),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge),
    ]);
    eprintln!("Spotify login: {url}\nWaiting for browser authorization; Ctrl+C cancels.");
    open_browser(url.as_str());
    let wait = async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut bytes = Vec::new();
            let read = tokio::time::timeout(Duration::from_secs(5), async {
                let mut limited = (&mut stream).take(8193);
                loop {
                    let byte = limited.read_u8().await?;
                    bytes.push(byte);
                    if bytes.ends_with(b"\r\n\r\n") || bytes.len() > 8192 {
                        break;
                    }
                }
                Ok::<_, std::io::Error>(())
            })
            .await;
            if !matches!(read, Ok(Ok(()))) || bytes.len() > 8192 {
                continue;
            }
            let request = std::str::from_utf8(&bytes)?;
            let mut line = request.lines().next().unwrap_or("").split_whitespace();
            let method = line.next();
            let target = line.next().unwrap_or("");
            let result = if method == Some("GET") {
                callback_code(target, &state)
            } else {
                Err("Expected GET callback".into())
            };
            let message = if result.is_ok() {
                "Spotify authorization received. You can close this window."
            } else {
                "Invalid Spotify callback. Return to the authorization window."
            };
            let status = if result.is_ok() {
                "200 OK"
            } else {
                "400 Bad Request"
            };
            let reply = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}", message.len());
            let _ =
                tokio::time::timeout(Duration::from_secs(5), stream.write_all(reply.as_bytes()))
                    .await;
            if let Ok(code) = result {
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(code);
            }
        }
    };
    let code = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(300), wait) => result??,
        _ = tokio::signal::ctrl_c() => return Err("Spotify login cancelled".into()),
    };
    let response = exchange(
        client,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &settings.redirect_uri),
            ("client_id", &settings.client_id),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    let tokens = Tokens {
        access_token: response.access_token,
        refresh_token: response
            .refresh_token
            .ok_or("Spotify did not provide a refresh token")?,
        expires_at: now().saturating_add(response.expires_in),
        scope: response.scope,
        client_id: settings.client_id.clone(),
    };
    tokens.save()?;
    eprintln!("Spotify session saved.");
    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
    #[cfg(not(windows))]
    for program in ["termux-open-url", "xdg-open", "open", "firefox", "chromium"] {
        if std::process::Command::new(program).arg(url).spawn().is_ok() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_matches_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn callbacks_require_state_and_unique_code() {
        assert_eq!(
            callback_code("/callback?code=valid&state=secret", "secret").unwrap(),
            "valid"
        );
        for uri in [
            "/callback?code=x",
            "/callback?code=x&state=wrong",
            "/other?code=x&state=secret",
            "/callback?code=x&code=y&state=secret",
            "/callback?error=access_denied&state=secret",
            "/callback?code=x&state=secret&state=secret",
        ] {
            assert!(callback_code(uri, "secret").is_err());
        }
    }
}
