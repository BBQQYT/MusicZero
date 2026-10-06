use crate::Result;
use serde::Deserialize;
use std::process::{Command, Output, Stdio};

#[derive(Deserialize)]
struct Sink {
    name: String,
    driver: String,
}

fn command(program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}. Install it with `pkg install pulseaudio`").into())
}

fn sinks() -> Result<Vec<Sink>> {
    let output = command("pactl", &["--format=json", "list", "sinks"])?;
    if !output.status.success() {
        return Err(format!(
            "PulseAudio: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn configured(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|value| !value.is_empty())
}

fn real_sink(sinks: &[Sink]) -> Option<&Sink> {
    sinks
        .iter()
        .find(|sink| sink.name != "auto_null" && !sink.driver.contains("module-null-sink"))
}

pub(super) fn prepare() -> Result<Option<String>> {
    // Connect first: launching a second daemon can collide with the existing
    // Termux socket. Respect explicitly configured servers and sinks.
    let available = match sinks() {
        Ok(sinks) => sinks,
        Err(error) if configured("PULSE_SERVER") => return Err(error),
        Err(_) => {
            let started = command("pulseaudio", &["--start", "--exit-idle-time=-1"])?;
            match sinks() {
                Ok(sinks) => sinks,
                Err(_) => {
                    log::warn!(
                        "PulseAudio default startup failed: {}",
                        String::from_utf8_lossy(&started.stderr).trim()
                    );
                    // Broken desktop/default OpenSL ES modules must not prevent
                    // connection. Start a minimal native server and add audio
                    // below; leave the user's default.pa untouched.
                    let minimal = command(
                        "pulseaudio",
                        &[
                            "--start",
                            "-n",
                            "--exit-idle-time=-1",
                            "--load=module-native-protocol-unix",
                        ],
                    )?;
                    sinks().map_err(|error| {
                        format!(
                            "PulseAudio startup failed: {} ({error})",
                            String::from_utf8_lossy(&minimal.stderr).trim()
                        )
                    })?
                }
            }
        }
    };
    if configured("PULSE_SERVER") || configured("PULSE_SINK") {
        return Ok(None);
    }
    if let Some(sink) = real_sink(&available) {
        let default = command("pactl", &["get-default-sink"])?;
        if default.status.success() {
            let name = String::from_utf8_lossy(&default.stdout);
            if available.iter().any(|sink| {
                sink.name == name.trim() && real_sink(std::slice::from_ref(sink)).is_some()
            }) {
                return Ok(None);
            }
        }
        // Do not change the server's default: route only this player away from
        // an automatic dummy sink when Android hardware is already available.
        return Ok(Some(sink.name.clone()));
    }
    let mut errors = Vec::new();
    for (module, name) in [
        ("module-aaudio-sink", "musiczero_aaudio"),
        ("module-sles-sink", "musiczero_sles"),
    ] {
        let argument = format!("sink_name={name}");
        let loaded = command("pactl", &["load-module", module, &argument])?;
        let available = sinks()?;
        if let Some(sink) = real_sink(&available) {
            log::info!("Android audio output: {}", sink.name);
            return Ok(Some(sink.name.clone()));
        }
        errors.push(format!(
            "{module}: {}",
            String::from_utf8_lossy(&loaded.stderr).trim()
        ));
    }
    Err(format!(
        "PulseAudio has no Android audio output. {}. Run `pkg upgrade pulseaudio` and retry.",
        errors.join("; ")
    )
    .into())
}
