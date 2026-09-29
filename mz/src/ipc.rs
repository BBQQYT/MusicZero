use serde_json::{json, Value};
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct ControlRequest {
    pub request: Value,
    pub answer: oneshot::Sender<Value>,
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
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
    writer
        .write_all(serde_json::to_string(request)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await?;
    if line.is_empty() {
        return Err("Host closed control channel".into());
    }
    Ok(serde_json::from_str(&line)?)
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
    BufReader::new(reader).read_line(&mut line).await?;
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(request) => {
            let (answer, rx) = oneshot::channel();
            tx.send(ControlRequest { request, answer }).await?;
            rx.await.unwrap_or_else(|_| json!({"error":"Host stopped"}))
        }
        Err(error) => json!({"error":error.to_string()}),
    };
    writer
        .write_all(serde_json::to_string(&response)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

pub async fn listen(tx: mpsc::Sender<ControlRequest>) -> Result<tokio::task::JoinHandle<()>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = socket_path();
        std::fs::create_dir_all(path.parent().ok_or("Invalid socket path")?)?;
        if path.exists() {
            if tokio::net::UnixStream::connect(&path).await.is_ok() {
                return Err("MusicZero is already running".into());
            }
            std::fs::remove_file(&path)?;
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        tokio::spawn(async move {
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
            loop {
                if let Err(error) = server.connect().await {
                    log::error!("Control pipe: {error}");
                    break;
                }
                let tx = tx.clone();
                tokio::spawn(async move {
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
