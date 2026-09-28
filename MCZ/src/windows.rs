//! Local control channel for Windows, where a session D-Bus is unavailable.
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

#[derive(Default, Deserialize, Serialize)]
pub struct Request {
    pub action: String,
    pub value: String,
    pub key: String,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Response {
    pub error: Option<String>,
    pub status: String,
    pub artist: String,
    pub title: String,
    pub playlist_id: String,
    pub playlists: Vec<(String, String)>,
    pub wave: Option<(String, String, String)>,
}

fn pipe_name(service: &str) -> io::Result<&'static str> {
    match service {
        "ymz" => Ok(r"\\.\pipe\musiczero-ymz"),
        "youmz" => Ok(r"\\.\pipe\musiczero-youmz"),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown service",
        )),
    }
}

pub async fn call(service: &str, request: Request) -> io::Result<Response> {
    let name = pipe_name(service)?;
    let mut retries = 0;
    let pipe = loop {
        match ClientOptions::new().open(name) {
            Ok(pipe) => break pipe,
            Err(error) if error.raw_os_error() == Some(231) && retries < 10 => {
                retries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(error) => return Err(error),
        }
    };
    let (reader, mut writer) = tokio::io::split(pipe);
    writer.write_all(&serde_json::to_vec(&request)?).await?;
    writer.write_all(b"\n").await?;
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await?;
    if line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "player closed control pipe",
        ));
    }
    let response: Response = serde_json::from_str(&line)?;
    if let Some(error) = &response.error {
        return Err(io::Error::new(io::ErrorKind::Other, error.clone()));
    }
    Ok(response)
}

pub async fn serve<F, Fut>(service: &str, handle: F) -> io::Result<()>
where
    F: Fn(Request) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    let name = pipe_name(service)?;
    let mut first = true;
    loop {
        let mut options = ServerOptions::new();
        if first {
            options.first_pipe_instance(true);
            first = false;
        }
        let pipe = options.create(name)?;
        pipe.connect().await?;
        let handle = handle.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(pipe);
            let mut line = String::new();
            if BufReader::new(reader).read_line(&mut line).await.is_err() {
                return;
            }
            let response = match serde_json::from_str::<Request>(&line) {
                Ok(request) => handle(request).await,
                Err(error) => Response {
                    error: Some(error.to_string()),
                    ..Response::default()
                },
            };
            if let Ok(mut data) = serde_json::to_vec(&response) {
                data.push(b'\n');
                let _ = writer.write_all(&data).await;
            }
        });
    }
}
