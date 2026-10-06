use crate::Result;
use rodio::mixer::Mixer;

#[cfg(not(any(target_os = "android", feature = "pulse-output")))]
pub struct Output(rodio::OutputStream);

#[cfg(not(any(target_os = "android", feature = "pulse-output")))]
impl Output {
    pub fn open() -> Result<Self> {
        Ok(Self(rodio::OutputStreamBuilder::open_default_stream()?))
    }
    pub fn mixer(&self) -> &Mixer {
        self.0.mixer()
    }
    pub fn check(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(any(target_os = "android", feature = "pulse-output"))]
pub use pulse::Output;

#[cfg(any(target_os = "android", feature = "pulse-output"))]
mod pulse {
    mod termux;

    use super::*;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    pub struct Output {
        mixer: Mixer,
        child: Child,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
        failures: mpsc::Receiver<String>,
    }

    impl Output {
        pub fn open() -> Result<Self> {
            // Termux has no Java VM context for CPAL. Use its native PulseAudio
            // client, feeding the same Rodio mixer so seeking/history stay shared.
            let device =
                if cfg!(target_os = "android") || std::env::var_os("TERMUX_VERSION").is_some() {
                    termux::prepare()?
                } else {
                    None
                };
            let mut client = Command::new("pacat");
            client.args([
                "--playback",
                "--raw",
                "--format=float32le",
                "--rate=48000",
                "--channels=2",
                "--latency-msec=40",
                "--process-time-msec=10",
            ]);
            if let Some(device) = device {
                client.arg(format!("--device={device}"));
            }
            // Bionic's iconv rejects the empty charset used by pacat's name
            // options AND its default media-name fallback. A filename supplies
            // the media name directly; this file is still our stdin pipe.
            // The context's environment properties do not use locale conversion.
            let mut child = client
                .arg("/proc/self/fd/0")
                .env("PULSE_PROP_OVERRIDE_application.name", "MusicZero")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| format!("pacat: {e}. Install the PulseAudio client"))?;
            let mut writer = child.stdin.take().expect("piped audio input");
            let (mixer, mut source) = rodio::mixer::mixer(2, 48000);
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let (failed, failures) = mpsc::channel();
            let mut output = Self {
                mixer,
                child,
                stop,
                worker: None,
                failures,
            };
            output.worker = Some(std::thread::Builder::new().name("mz-pulse".into()).spawn(
                move || {
                    let frame = Duration::from_millis(10);
                    let mut deadline = Instant::now();
                    let mut pcm = [0u8; 960 * 4];
                    while !flag.load(Ordering::Relaxed) {
                        for bytes in pcm.as_chunks_mut::<4>().0 {
                            bytes.copy_from_slice(&source.next().unwrap_or(0.0).to_le_bytes());
                        }
                        if let Err(error) = writer.write_all(&pcm) {
                            let _ = failed.send(format!("PulseAudio output failed: {error}"));
                            return;
                        }
                        // Bound mixer read-ahead even if the client's pipe accepts
                        // seconds of PCM. Never catch up after a blocked write.
                        deadline += frame;
                        let now = Instant::now();
                        if deadline > now {
                            std::thread::sleep(deadline - now);
                        } else {
                            deadline = now;
                        }
                    }
                },
            )?);
            log::info!("Audio output: PulseAudio (48 kHz stereo)");
            Ok(output)
        }

        pub fn mixer(&self) -> &Mixer {
            &self.mixer
        }

        pub fn check(&mut self) -> Result<()> {
            if let Some(status) = self.child.try_wait()? {
                return Err(format!("PulseAudio client exited: {status}").into());
            }
            match self.failures.try_recv() {
                Ok(error) => Err(error.into()),
                Err(mpsc::TryRecvError::Disconnected) => Err("PulseAudio writer stopped".into()),
                Err(mpsc::TryRecvError::Empty) => Ok(()),
            }
        }
    }

    impl Drop for Output {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            // Killing the client also releases a writer blocked on its pipe.
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
