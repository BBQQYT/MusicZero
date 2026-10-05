//! Ordered provider lifecycle notifications, kept off the playback/control loop.
use crate::plugin::{Module, Result, Track};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

struct Event {
    module: Module,
    track_id: String,
    context: String,
    kind: &'static str,
    played: Duration,
}
enum Request {
    Event(Event),
    Barrier(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct Sender(mpsc::Sender<Request>);
impl Sender {
    pub fn emit(&self, module: Module, track: &Track, kind: &'static str, played: Duration) {
        if track.feedback.is_empty() {
            return;
        }
        if track.feedback.len() > 4096 || track.feedback.chars().any(char::is_control) {
            log::warn!("Invalid feedback context from {}", module.manifest.id);
            return;
        }
        let event = Event {
            module,
            track_id: track.id.clone(),
            context: track.feedback.clone(),
            kind,
            played,
        };
        if self.0.try_send(Request::Event(event)).is_err() {
            log::warn!("Provider feedback queue is full or stopped");
        }
    }

    pub async fn drain(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let (tx, rx) = oneshot::channel();
            self.0.send(Request::Barrier(tx)).await?;
            rx.await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await?
    }
}

pub struct Worker {
    pub sender: Sender,
    task: tokio::task::JoinHandle<()>,
}
impl Worker {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel(128);
        let task = tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                match request {
                    Request::Barrier(tx) => {
                        let _ = tx.send(());
                    }
                    Request::Event(event) => {
                        let seconds = event.played.as_secs_f64().to_string();
                        let result = tokio::time::timeout(
                            Duration::from_secs(5),
                            event.module.json(
                                "feedback",
                                &[event.kind, &event.track_id, &event.context, &seconds],
                            ),
                        )
                        .await;
                        match result {
                            Ok(Ok(_)) => {}
                            Ok(Err(error)) => {
                                log::warn!("{} feedback: {error}", event.module.manifest.id)
                            }
                            Err(_) => log::warn!("{} feedback timed out", event.module.manifest.id),
                        }
                    }
                }
            }
        });
        Self {
            sender: Sender(tx),
            task,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.task.abort();
    }
}
