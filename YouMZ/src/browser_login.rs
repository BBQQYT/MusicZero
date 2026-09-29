use crate::api::YtClient;
use crate::config::Config;
use data_encoding::BASE64;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Copy)]
enum Kind {
    Firefox,
    Chromium,
}

struct Browser {
    path: PathBuf,
    kind: Kind,
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let filename = if cfg!(windows) && !name.to_ascii_lowercase().ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(&filename))
            .find(|candidate| candidate.is_file())
    })
}

fn find_browser() -> Result<Browser> {
    if let Some(path) = env::var_os("YOUMZ_BROWSER") {
        let path = PathBuf::from(path);
        let browser_path = find_on_path(&path.to_string_lossy())
            .ok_or("YOUMZ_BROWSER не указывает на исполняемый браузер")?;
        let filename = browser_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let kind = if filename.contains("firefox")
            || filename.contains("librefox")
            || filename.contains("librewolf")
        {
            Kind::Firefox
        } else if filename.contains("chrom") {
            Kind::Chromium
        } else {
            return Err(
                "YOUMZ_BROWSER поддерживает Firefox, LibreWolf, Chromium или Chrome".into(),
            );
        };
        return Ok(Browser {
            path: browser_path,
            kind,
        });
    }
    for (name, kind) in [
        ("firefox", Kind::Firefox),
        ("firefox-esr", Kind::Firefox),
        ("librefox", Kind::Firefox),
        ("librewolf", Kind::Firefox),
        ("chromium", Kind::Chromium),
        ("chromium-browser", Kind::Chromium),
        ("google-chrome", Kind::Chromium),
        ("google-chrome-stable", Kind::Chromium),
        ("chrome", Kind::Chromium),
    ] {
        if let Some(path) = find_on_path(name) {
            return Ok(Browser { path, kind });
        }
    }
    #[cfg(windows)]
    for (root, relative, kind) in [
        ("PROGRAMFILES", "Mozilla Firefox/firefox.exe", Kind::Firefox),
        (
            "PROGRAMFILES(X86)",
            "Mozilla Firefox/firefox.exe",
            Kind::Firefox,
        ),
        ("PROGRAMFILES", "LibreWolf/librewolf.exe", Kind::Firefox),
        (
            "LOCALAPPDATA",
            "Programs/LibreWolf/librewolf.exe",
            Kind::Firefox,
        ),
        (
            "PROGRAMFILES",
            "Google/Chrome/Application/chrome.exe",
            Kind::Chromium,
        ),
        (
            "PROGRAMFILES(X86)",
            "Google/Chrome/Application/chrome.exe",
            Kind::Chromium,
        ),
        (
            "LOCALAPPDATA",
            "Google/Chrome/Application/chrome.exe",
            Kind::Chromium,
        ),
        (
            "PROGRAMFILES",
            "Chromium/Application/chrome.exe",
            Kind::Chromium,
        ),
    ] {
        if let Some(base) = env::var_os(root) {
            let path = PathBuf::from(base).join(relative);
            if path.is_file() {
                return Ok(Browser { path, kind });
            }
        }
    }
    Err("Не найден Firefox, LibreWolf, Chromium или Chrome; задайте YOUMZ_BROWSER".into())
}

async fn launch(browser: &Browser, profile: &TempDir) -> Result<(Child, Option<u16>)> {
    let mut command = Command::new(&browser.path);
    let port = match browser.kind {
        Kind::Firefox => {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port();
            drop(listener);
            command
                .arg("--no-remote")
                .arg("--new-instance")
                .arg("--profile")
                .arg(profile.path())
                .arg("--remote-debugging-port")
                .arg(port.to_string());
            Some(port)
        }
        Kind::Chromium => {
            command
                .arg(format!("--user-data-dir={}", profile.path().display()))
                .arg("--remote-debugging-port=0")
                .arg("--no-first-run")
                .arg("--no-default-browser-check");
            None
        }
    };
    command
        .arg("https://music.youtube.com/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    Ok((command.spawn()?, port))
}

async fn connect(
    browser: &Browser,
    profile: &TempDir,
    child: &mut Child,
    port: Option<u16>,
) -> Result<Socket> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("Браузер завершился до открытия страницы: {status}").into());
        }
        let url = match browser.kind {
            Kind::Firefox => Some(format!(
                "ws://127.0.0.1:{}/session",
                port.ok_or("Нет порта Firefox")?
            )),
            Kind::Chromium => std::fs::read_to_string(profile.path().join("DevToolsActivePort"))
                .ok()
                .and_then(|content| {
                    let mut lines = content.lines();
                    let port = lines.next()?.parse::<u16>().ok()?;
                    let path = lines.next()?;
                    Some(format!("ws://127.0.0.1:{port}{path}"))
                }),
        };
        if let Some(url) = url {
            if let Ok(Ok((socket, _))) = tokio::time::timeout(
                Duration::from_secs(2),
                tokio_tungstenite::connect_async(url.as_str()),
            )
            .await
            {
                return Ok(socket);
            }
        }
        if Instant::now() >= deadline {
            return Err("Браузер не открыл интерфейс входа за 30 секунд".into());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn command(socket: &mut Socket, id: u64, method: &str, params: Value) -> Result<Value> {
    socket
        .send(Message::Text(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .into(),
        ))
        .await?;
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await?
            .ok_or("Браузер закрыл соединение")??;
        if !message.is_text() {
            continue;
        }
        let response: Value = serde_json::from_str(message.to_text()?)?;
        if response["id"].as_u64() != Some(id) {
            continue;
        }
        if response.get("error").is_some() || response["type"] == "error" {
            return Err(format!(
                "Браузер отклонил {method}: {}",
                response["message"].as_str().unwrap_or("неизвестная ошибка")
            )
            .into());
        }
        return Ok(response["result"].clone());
    }
}

fn cookie_value(cookie: &Value) -> Option<String> {
    if let Some(value) = cookie["value"].as_str() {
        return Some(value.to_owned());
    }
    let value = &cookie["value"];
    if value["type"] == "string" {
        return value["value"].as_str().map(str::to_owned);
    }
    if value["type"] == "base64" {
        return BASE64
            .decode(value["value"].as_str()?.as_bytes())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
    }
    None
}

fn session_from_browser(result: &Value) -> Option<String> {
    let cookies = result["cookies"].as_array()?;
    let mut values = BTreeMap::new();
    for cookie in cookies {
        let Some(domain) = cookie["domain"].as_str() else {
            continue;
        };
        let domain = domain.trim_start_matches('.');
        if domain != "youtube.com" && !domain.ends_with(".youtube.com") {
            continue;
        }
        let Some(name) = cookie["name"].as_str() else {
            continue;
        };
        let Some(value) = cookie_value(cookie) else {
            continue;
        };
        if !name.is_empty()
            && !value.is_empty()
            && !name.contains(';')
            && !name.contains('=')
            && !value.chars().any(|c| matches!(c, ';' | '\n' | '\r'))
        {
            values.insert(name.to_owned(), value);
        }
    }
    if !values.contains_key("SID")
        || !(values.contains_key("SAPISID") || values.contains_key("__Secure-3PAPISID"))
    {
        return None;
    }
    Some(
        values
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

async fn capture(
    browser: &Browser,
    profile: &TempDir,
    child: &mut Child,
    port: Option<u16>,
    proxy: &Option<String>,
) -> Result<String> {
    let mut socket = connect(browser, profile, child, port).await?;
    let mut id = 1;
    if matches!(browser.kind, Kind::Firefox) {
        command(&mut socket, id, "session.new", json!({"capabilities":{}})).await?;
        id += 1;
    }
    eprintln!(
        "Войдите в YouTube Music в открывшемся окне. Ожидаю завершения входа (Ctrl+C — отмена)..."
    );
    let mut last_attempt = String::new();
    let mut last_attempt_at = Instant::now() - Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            return Err("Окно входа закрыто".into());
        }
        let method = if matches!(browser.kind, Kind::Firefox) {
            "storage.getCookies"
        } else {
            "Storage.getCookies"
        };
        let result = command(&mut socket, id, method, json!({})).await?;
        id += 1;
        if let Some(session) = session_from_browser(&result) {
            if session != last_attempt || last_attempt_at.elapsed() >= Duration::from_secs(10) {
                last_attempt = session.clone();
                last_attempt_at = Instant::now();
                let config = Arc::new(Config {
                    cookie: Some(session.clone()),
                    proxy: proxy.clone(),
                });
                let client = YtClient::new(config).await;
                if client.list_playlists().await.is_ok() {
                    return Ok(session);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn login(proxy: &Option<String>) -> Result<String> {
    let browser = find_browser()?;
    let profile = TempDir::new()?;
    let (mut child, port) = launch(&browser, &profile).await?;
    let result = tokio::select! {
        result = capture(&browser, &profile, &mut child, port, proxy) => result,
        _ = tokio::signal::ctrl_c() => Err("Вход отменён".into()),
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}
