//! Optional Termux notification controls. API failures never stop playback.
#[derive(Clone, Default, PartialEq)]
pub struct State {
    pub title: String,
    pub artist: String,
    pub source: String,
    pub playing: bool,
}

#[cfg(any(target_os = "android", feature = "aaudio-output"))]
mod termux {
    use super::State;
    use crate::config::{Language, Settings};
    use std::{path::PathBuf, process::Stdio, time::Duration};
    use tokio::{
        process::Command,
        sync::{oneshot, watch},
        task::JoinHandle,
    };

    const ID: &str = "musiczero-player";

    pub struct Controls {
        sender: Option<watch::Sender<State>>,
        worker: Option<JoinHandle<()>>,
        stop: Option<oneshot::Sender<()>>,
    }

    fn quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    // Termux runs notification actions in a fresh shell, without our environment.
    fn action(executable: &str, command: &str) -> String {
        let mut script = String::new();
        for key in [
            "HOME",
            "XDG_RUNTIME_DIR",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "TMPDIR",
        ] {
            if let Ok(value) = std::env::var(key) {
                script.push_str(&format!("export {key}={}; ", quote(&value)));
            }
        }
        script.push_str(&format!(
            "exec {} {command} >/dev/null 2>&1",
            quote(executable)
        ));
        script
    }

    struct ProcessGroup(Option<u32>);
    impl Drop for ProcessGroup {
        fn drop(&mut self) {
            if let Some(pid) = self.0 {
                // SAFETY: this is the private process group created for our child.
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
        }
    }

    async fn execute(path: &PathBuf, args: &[String]) -> crate::Result<()> {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(path);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // The API shell script starts a native child. Kill the entire process group
        // on timeout, including that child, rather than leaving a blocked API call.
        command.as_std_mut().process_group(0);
        let mut child = command.spawn()?;
        let mut group = ProcessGroup(child.id());
        match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
            Ok(result) => {
                group.0 = None;
                if result?.success() {
                    Ok(())
                } else {
                    Err("Termux:API returned an error".into())
                }
            }
            Err(_) => {
                drop(group);
                let _ = child.wait().await;
                Err("Termux:API timed out".into())
            }
        }
    }

    fn arguments(state: &State, language: Language, executable: &str) -> Vec<String> {
        let title = if state.title.is_empty() {
            "MusicZero"
        } else {
            &state.title
        };
        let status = if state.playing {
            language.text("Воспроизведение", "Playing")
        } else {
            language.text("Пауза", "Paused")
        };
        let content = format!("{} · {} · {status}", state.artist, state.source);
        [
            "--id",
            ID,
            "--ongoing",
            "--alert-once",
            "--priority",
            "low",
            "--title",
            title,
            "--content",
            &content,
            "--button1",
            language.text("◀ Назад", "◀ Previous"),
            "--button1-action",
            &action(executable, "prev"),
            "--button2",
            if state.playing {
                language.text("Ⅱ Пауза", "Ⅱ Pause")
            } else {
                language.text("▶ Продолжить", "▶ Play")
            },
            "--button2-action",
            &action(executable, "toggle"),
            "--button3",
            language.text("Вперёд ▶", "Next ▶"),
            "--button3-action",
            &action(executable, "next"),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    impl Controls {
        pub fn new() -> Self {
            let disabled = Self {
                sender: None,
                worker: None,
                stop: None,
            };
            let Ok(settings) = Settings::load() else {
                return disabled;
            };
            if !settings.notifications_enabled {
                return disabled;
            }
            let Some(prefix) = std::env::var_os("PREFIX") else {
                return disabled;
            };
            let bin = PathBuf::from(prefix).join("bin");
            let show = bin.join("termux-notification");
            let remove = bin.join("termux-notification-remove");
            if !show.is_file() || !remove.is_file() {
                log::info!("Android controls: install the Termux:API app and run `pkg install termux-api`.");
                return disabled;
            }
            let Ok(executable) = std::env::current_exe() else {
                return disabled;
            };
            let Some(executable) = executable.to_str().map(str::to_owned) else {
                return disabled;
            };
            let (sender, mut receiver) = watch::channel(State::default());
            let (stop, stopped) = oneshot::channel();
            let worker = tokio::spawn(async move {
                let updates = async {
                    let mut warned = false;
                    while receiver.changed().await.is_ok() {
                        let state = receiver.borrow_and_update().clone();
                        if let Err(error) =
                            execute(&show, &arguments(&state, settings.language, &executable)).await
                        {
                            if !warned {
                                log::warn!("Android controls unavailable: {error}. Check Termux:API and notification permissions; audio continues.");
                                warned = true;
                            }
                            // Bound retries if the companion app is missing or unresponsive.
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                };
                tokio::select! { _ = updates => {}, _ = stopped => {} }
                if let Err(error) = execute(&remove, &[ID.to_owned()]).await {
                    log::debug!("Remove Android controls: {error}");
                }
            });
            Self {
                sender: Some(sender),
                worker: Some(worker),
                stop: Some(stop),
            }
        }

        pub fn update(&self, state: State) {
            if let Some(sender) = &self.sender {
                sender.send_if_modified(|old| {
                    if *old == state {
                        false
                    } else {
                        *old = state;
                        true
                    }
                });
            }
        }

        pub async fn close(mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            self.sender.take();
            if let Some(worker) = self.worker.take() {
                let _ = worker.await;
            }
        }
    }
}

#[cfg(any(target_os = "android", feature = "aaudio-output"))]
pub use termux::Controls;

#[cfg(not(any(target_os = "android", feature = "aaudio-output")))]
pub struct Controls;
#[cfg(not(any(target_os = "android", feature = "aaudio-output")))]
impl Controls {
    pub fn new() -> Self {
        Self
    }
    pub fn update(&self, _: State) {}
    pub async fn close(self) {}
}
