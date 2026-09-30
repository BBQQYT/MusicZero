//! Bounded PCM radio playback; audio is never accumulated into a temporary file.
use crate::plugin::{Module, Result};
use rodio::Source;
use std::collections::VecDeque;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

pub struct LiveSource {
    receiver: mpsc::Receiver<Vec<f32>>,
    samples: VecDeque<f32>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for LiveSource {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Iterator for LiveSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(sample) = self.samples.pop_front() {
            return Some(sample);
        }
        match self.receiver.try_recv() {
            Ok(chunk) => {
                self.samples.extend(chunk);
                self.samples.pop_front()
            }
            // Never block the audio device thread on network I/O.
            Err(mpsc::error::TryRecvError::Empty) => Some(0.0),
            Err(mpsc::error::TryRecvError::Disconnected) => None,
        }
    }
}
impl Source for LiveSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        48000
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
impl LiveSource {
    pub async fn open(module: Module, id: String, buffer_ms: u32) -> Result<Self> {
        let chunks = buffer_ms.clamp(250, 10000).div_ceil(20) as usize;
        let (sender, receiver) = mpsc::channel(chunks);
        let mut child = tokio::process::Command::new(&module.executable)
            .arg("audio")
            .arg(id)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdout = child.stdout.take().ok_or("Radio stdout unavailable")?;
        let task = tokio::spawn(async move {
            let mut buffer = [0u8; 3840]; // 20 ms of stereo 48 kHz signed 16-bit PCM.
            let mut used = 0;
            loop {
                match stdout.read(&mut buffer[used..]).await {
                    Ok(0) => {
                        if used >= 2 {
                            let _ = sender.send(pcm(&buffer[..used - used % 2])).await;
                        }
                        break;
                    }
                    Ok(n) => {
                        used += n;
                        if used < buffer.len() {
                            continue;
                        }
                    }
                    Err(error) => {
                        log::warn!("Радиопоток: {error}");
                        break;
                    }
                }
                if sender.send(pcm(&buffer)).await.is_err() {
                    return;
                }
                used = 0;
            }
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) if !status.success() => {
                    log::warn!("Радиомодуль завершился: {status}")
                }
                _ => {}
            }
        });
        let mut source = Self {
            receiver,
            samples: VecDeque::new(),
            task,
        };
        tokio::time::timeout(Duration::from_secs(60), async {
            for _ in 0..chunks {
                match source.receiver.recv().await {
                    Some(chunk) => source.samples.extend(chunk),
                    None => break,
                }
            }
        })
        .await?;
        if source.samples.is_empty() {
            return Err("Радиостанция не вернула аудио".into());
        }
        Ok(source)
    }
}
fn pcm(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn starvation_is_silence_and_eof_ends_the_source() {
        let (tx, receiver) = mpsc::channel(1);
        let mut source = LiveSource {
            receiver,
            samples: VecDeque::new(),
            task: tokio::spawn(std::future::pending()),
        };
        assert_eq!(source.next(), Some(0.0));
        tx.send(pcm(&[0, 128, 255, 127])).await.unwrap();
        assert_eq!(source.next(), Some(-1.0));
        assert!(source.next().unwrap() > 0.99);
        drop(tx);
        assert_eq!(source.next(), None);
    }
}

#[cfg(all(test, unix))]
mod process_tests {
    use super::*;
    use crate::plugin::Manifest;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn live_audio_starts_before_eof_and_drop_kills_provider() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pcm"), vec![0u8; 3840 * 20]).unwrap();
        let executable = dir.path().join("radio");
        std::fs::write(
            &executable,
            b"#!/bin/sh\ncd \"$(dirname \"$0\")\"\necho $$ > pid\ncat pcm\nexec sleep 600\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let module = Module {
            executable,
            manifest: Manifest {
                protocol: 1,
                id: "test".into(),
                name: "Test".into(),
                binary: "radio".into(),
                default_playlist: "radio".into(),
            },
        };
        let source = tokio::time::timeout(
            Duration::from_secs(5),
            LiveSource::open(module, "radio".into(), 250),
        )
        .await
        .unwrap()
        .unwrap();
        let pid = std::fs::read_to_string(dir.path().join("pid")).unwrap();
        let pid = pid.trim();
        assert!(std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .unwrap()
            .success());
        drop(source);
        for _ in 0..100 {
            if !std::process::Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("Radio provider was left running");
    }
}
