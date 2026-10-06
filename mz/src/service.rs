use crate::{config::Settings, ipc, plugin, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const UNIT: &str = "musiczero.service";

pub fn supported() -> bool {
    cfg!(target_os = "linux")
}

pub fn path() -> PathBuf {
    mcz::paths::config_dir("mz")
        .parent()
        .unwrap_or(Path::new("."))
        .join("systemd/user")
        .join(UNIT)
}

fn quote(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        return Err("Service paths and environment must not contain control characters".into());
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}

fn unit(executable: &Path, module: &str, env: &[(String, String)]) -> Result<String> {
    let executable = executable
        .to_str()
        .ok_or("Service executable must have a UTF-8 path")?;
    // systemd rejects special characters in its executable path. This fixed
    // shell program only execs separately quoted argv; paths never become shell
    // code. ':' preserves literal dollars. Percent specifiers are escaped.
    let mut text = format!("[Unit]\nDescription=MusicZero player\nAfter=graphical-session-pre.target\n\n[Service]\nType=simple\nExecStart=:/bin/sh -c \"exec \\\"$@\\\"\" musiczero {} serve {}\nRestart=on-failure\nRestartSec=5\nTimeoutStopSec=10\n", quote(executable)?, quote(module)?);
    for (key, value) in env {
        text.push_str(&format!(
            "Environment={}\n",
            quote(&format!("{key}={value}"))?
        ));
    }
    text.push_str("\n[Install]\nWantedBy=default.target\n");
    Ok(text)
}

fn write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid service file path")?;
    std::fs::create_dir_all(parent)?;
    if path.is_symlink() || (path.exists() && !path.is_file()) {
        return Err("Service file must be a regular file".into());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

async fn systemctl(args: &[&str]) -> Result<String> {
    if !supported() {
        return Err("Сервисы MusicZero поддерживаются в Linux с systemd --user".into());
    }
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("systemctl")
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "systemctl --user timed out")??;
    if !output.status.success() {
        return Err(format!(
            "systemctl --user: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub async fn status() -> Result<Value> {
    if !supported() {
        return Err("Сервисы MusicZero поддерживаются только в Linux".into());
    }
    if !path().exists() {
        return Ok(json!({"installed":false,"active":false,"enabled":false}));
    }
    let output = systemctl(&[
        "show",
        UNIT,
        "--property=ActiveState,UnitFileState",
        "--no-pager",
    ])
    .await?;
    Ok(
        json!({"installed":true,"active":output.lines().any(|line| line=="ActiveState=active"),
        "enabled":output.lines().any(|line| line=="UnitFileState=enabled"),"details":output}),
    )
}

pub async fn install(module: &plugin::Module, settings: &mut Settings) -> Result<PathBuf> {
    if !supported() {
        return Err("Сервисы MusicZero поддерживаются только в Linux".into());
    }
    module.validate().await?;
    let mut updated = settings.clone();
    updated.set("service_module", &module.manifest.id)?;
    let executable = std::env::current_exe()?;
    let mut env: Vec<(String, String)> = [
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "MZ_MODULES_DIR",
        "RUST_LOG",
        "TMPDIR",
        "TMP",
        "TEMP",
        "YOUMZ_BROWSER",
    ]
    .iter()
    .filter_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| (key.to_string(), value))
    })
    .collect();
    let bin_dir = executable
        .parent()
        .ok_or("Executable parent missing")?
        .to_str()
        .ok_or("Executable path must be UTF-8")?;
    env.push((
        "PATH".into(),
        format!(
            "{bin_dir}:{}",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into())
        ),
    ));
    let text = unit(&executable, &module.manifest.id, &env)?;
    let path = path();
    if path.is_symlink() {
        return Err("Service file must be a regular file".into());
    }
    let previous = if path.exists() {
        Some(std::fs::read(&path)?)
    } else {
        None
    };
    // Check the user manager before creating a unit. No root service or sudo is used.
    systemctl(&["daemon-reload"]).await?;
    let was_enabled = previous.is_some() && status().await?["enabled"].as_bool().unwrap_or(false);
    write(&path, text.as_bytes())?;
    let result = async {
        systemctl(&["daemon-reload"]).await?;
        systemctl(&["enable", path.to_str().ok_or("Service path must be UTF-8")?]).await?;
        updated.save()?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    if let Err(error) = result {
        if !was_enabled {
            let _ = systemctl(&["disable", UNIT]).await;
        }
        if let Some(previous) = previous {
            write(&path, &previous)?;
        } else {
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        let _ = systemctl(&["daemon-reload"]).await;
        return Err(error);
    }
    *settings = updated;
    Ok(path)
}

pub async fn action(action: &str) -> Result<()> {
    if !supported() {
        return Err("Сервисы MusicZero поддерживаются только в Linux".into());
    }
    if !path().is_file() {
        return Err("Сервис не установлен. Выполните `mz service install <модуль>`.".into());
    }
    match action {
        "start" | "restart" => {
            if ipc::call(&json!({"action":"ping"})).await.is_ok()
                && !status().await?["active"].as_bool().unwrap_or(false)
            {
                return Err(
                    "Плеер уже запущен вручную. Выполните `mz quit`, затем `mz service start`."
                        .into(),
                );
            }
            systemctl(&[action, UNIT]).await?;
        }
        "stop" => {
            systemctl(&["stop", UNIT]).await?;
        }
        "remove" => {
            systemctl(&["disable", "--now", UNIT]).await?;
            if path().exists() {
                std::fs::remove_file(path())?;
            }
            systemctl(&["daemon-reload"]).await?;
        }
        _ => return Err("Unknown service action".into()),
    }
    Ok(())
}

pub async fn cli(args: &[String]) -> Result<()> {
    let mut settings = Settings::load()?;
    match args.first().map(String::as_str) {
        Some("install") => {
            let modules = plugin::discover()?;
            let module = crate::module(&modules, args.get(1).ok_or("Укажите модуль")?)?;
            let path = install(module, &mut settings).await?;
            println!(
                "{}: {}",
                settings.language.text(
                    "Сервис установлен; автозапуск включён",
                    "Service installed; autostart enabled"
                ),
                path.display()
            );
            println!(
                "{}",
                settings.language.text(
                    "Для запуска сейчас: mz service start",
                    "To start now: mz service start"
                )
            );
        }
        Some("status") => println!("{}", serde_json::to_string_pretty(&status().await?)?),
        Some(operation @ ("start" | "stop" | "restart" | "remove")) => {
            action(operation).await?;
            println!("OK");
        }
        _ => {
            return Err("Используйте `mz service install|start|stop|restart|status|remove`".into())
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_quotes_paths_and_environment_without_shell_or_secret_expansion() {
        let text = unit(
            Path::new("/tmp/Музыка %u $HOME/\"mz\\"),
            "ymz",
            &[("PATH".into(), "/tmp/$foo %h/bin:/usr/bin".into())],
        )
        .unwrap();
        assert!(
            text.contains("musiczero \"/tmp/Музыка %%u $HOME/\\\"mz\\\\\" serve \"ymz\""),
            "{text}"
        );
        assert!(text.contains("Environment=\"PATH=/tmp/$foo %%h/bin:/usr/bin\""));
        assert!(quote("bad\npath").is_err());
    }
}
