use serde_json::{json, Value};
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct ControlRequest {
    pub request: Value,
    pub answer: oneshot::Sender<Value>,
}

#[cfg(unix)]
pub(crate) fn socket_path() -> std::path::PathBuf {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| mcz::paths::cache_dir("mz"));
    root.join("musiczero.sock")
}

#[cfg(windows)]
const PIPE: &str = r"\\.\pipe\musiczero-mz";

async fn exchange<S>(stream: S, request: &Value) -> Result<Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let payload = serde_json::to_string(request)?;
    if payload.len() > 64 * 1024 {
        return Err("Control request too large".into());
    }
    tokio::time::timeout(std::time::Duration::from_secs(100), async {
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        let mut line = String::new();
        let mut limited = BufReader::new(reader).take(4 * 1024 * 1024 + 1);
        let n = limited.read_line(&mut line).await?;
        if n == 0 {
            return Err::<_, Box<dyn Error + Send + Sync>>("Host closed control channel".into());
        }
        if line.len() > 4 * 1024 * 1024 {
            return Err("Control response too large".into());
        }
        Ok(serde_json::from_str::<Value>(&line)?)
    })
    .await?
}

pub async fn call(request: &Value) -> Result<Value> {
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path()).await?;
    #[cfg(windows)]
    let stream = {
        use tokio::net::windows::named_pipe::ClientOptions;
        let mut attempts = 0;
        loop {
            match ClientOptions::new().open(PIPE) {
                Ok(stream) => break stream,
                Err(error) if error.raw_os_error() == Some(231) && attempts < 10 => {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    };
    exchange(stream, request).await
}

async fn handle_stream<S>(stream: S, tx: mpsc::Sender<ControlRequest>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut line = String::new();
    let mut limited = BufReader::new(reader).take(64 * 1024 + 1);
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        limited.read_line(&mut line),
    )
    .await??;
    if n == 0 {
        return Ok(());
    }
    if line.len() > 64 * 1024 {
        let payload = serde_json::to_string(&json!({"error":"Control request too large"}))?;
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        return Ok(());
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(request) => {
            let (answer, rx) = oneshot::channel();
            if tx.try_send(ControlRequest { request, answer }).is_err() {
                json!({"error":"Host busy or stopped"})
            } else {
                tokio::time::timeout(std::time::Duration::from_secs(90), rx)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_else(|| json!({"error":"Host stopped"}))
            }
        }
        Err(error) => json!({"error":error.to_string()}),
    };
    let payload = serde_json::to_string(&response)?;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    })
    .await??;
    Ok(())
}

pub async fn listen(tx: mpsc::Sender<ControlRequest>) -> Result<tokio::task::JoinHandle<()>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};
        let path = socket_path();
        std::fs::create_dir_all(path.parent().ok_or("Invalid socket path")?)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err("Control path exists and is not a socket".into());
            }
            if tokio::net::UnixStream::connect(&path).await.is_ok() {
                return Err("MusicZero is already running".into());
            }
            std::fs::remove_file(&path)?;
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(tokio::spawn(async move {
            let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(32));
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let Ok(permit) = slots.clone().try_acquire_owned() else {
                            continue;
                        };
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(error) = handle_stream(stream, tx).await {
                                log::warn!("Control channel: {error}");
                            }
                        });
                    }
                    Err(error) => {
                        log::error!("Control socket: {error}");
                        break;
                    }
                }
            }
        }))
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let first = ServerOptions::new()
            .first_pipe_instance(true)
            .create(PIPE)?;
        Ok(tokio::spawn(async move {
            let mut server = first;
            let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(32));
            loop {
                if let Err(error) = server.connect().await {
                    log::error!("Control pipe: {error}");
                    break;
                }
                let permit = slots.clone().acquire_owned().await.expect("Semaphore open");
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(error) = handle_stream(server, tx).await {
                        log::warn!("Control channel: {error}");
                    }
                });
                match ServerOptions::new().create(PIPE) {
                    Ok(next) => server = next,
                    Err(error) => {
                        log::error!("Control pipe: {error}");
                        break;
                    }
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn large_playlist_response_survives_ipc() {
        let (client, server) = tokio::io::duplex(1024);
        let (tx, mut rx) = mpsc::channel(1);
        let handler = tokio::spawn(handle_stream(server, tx));
        let responder = tokio::spawn(async move {
            let request = rx.recv().await.unwrap();
            request
                .answer
                .send(json!({"playlists": [{"name": "x".repeat(100_000)}]}))
                .unwrap();
        });
        let response = exchange(client, &json!({"action":"playlists"}))
            .await
            .unwrap();
        assert_eq!(
            response["playlists"][0]["name"].as_str().unwrap().len(),
            100_000
        );
        responder.await.unwrap();
        handler.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn oversized_request_is_rejected_without_dispatch() {
        let (mut client, server) = tokio::io::duplex(1024);
        let (tx, mut rx) = mpsc::channel(1);
        let handler = tokio::spawn(handle_stream(server, tx));
        client.write_all(&vec![b'x'; 64 * 1024 + 1]).await.unwrap();
        let mut response = String::new();
        BufReader::new(client)
            .read_line(&mut response)
            .await
            .unwrap();
        assert!(response.contains("too large"));
        handler.await.unwrap().unwrap();
        assert!(rx.recv().await.is_none());
    }
}
