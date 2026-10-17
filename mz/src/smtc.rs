//! Windows System Media Transport Controls: global media keys, the taskbar
//! media flyout and lock screen controls. Commands travel through the same
//! IPC loop the Linux tray and CLI clients use, so no player changes are
//! needed. Failures are logged; audio playback never depends on them.
use crate::ipc;
use serde_json::json;
use std::sync::mpsc;
use std::time::Duration;
use windows::core::{factory, HSTRING, PCWSTR};
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WAIT_TIMEOUT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MsgWaitForMultipleObjectsEx,
    PeekMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, MSG, MWMO_INPUTAVAILABLE,
    PM_REMOVE, QUEUE_STATUS_FLAGS, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_QUIT,
    WNDCLASSW, WNDCLASS_STYLES,
};

const POLL: Duration = Duration::from_secs(1);
const QS_ALLINPUT: QUEUE_STATUS_FLAGS = QUEUE_STATUS_FLAGS(1279);

#[derive(Clone, Default, PartialEq)]
struct Snapshot {
    title: String,
    artist: String,
    source: String,
    status: String,
    can_previous: bool,
    position_ms: u64,
    duration_ms: u64,
    art_url: String,
}

enum Message {
    State(Snapshot),
}

fn snapshot(value: &serde_json::Value) -> Option<Snapshot> {
    Some(Snapshot {
        title: value["title"].as_str().unwrap_or("").into(),
        artist: value["artist"].as_str().unwrap_or("").into(),
        source: value["name"].as_str().unwrap_or("").into(),
        status: value["status"].as_str().unwrap_or("").into(),
        can_previous: value["can_previous"].as_bool().unwrap_or(false),
        position_ms: value["position_ms"].as_u64().unwrap_or(0),
        duration_ms: value["duration_ms"].as_u64().unwrap_or(0),
        art_url: value["art_url"].as_str().unwrap_or("").into(),
    })
}

fn button_to_action(button: SystemMediaTransportControlsButton) -> Option<String> {
    use SystemMediaTransportControlsButton as B;
    match button {
        B::Play => Some("play".into()),
        B::Pause => Some("pause".into()),
        B::Stop => Some("stop".into()),
        B::Next => Some("next".into()),
        B::Previous => Some("previous".into()),
        B::FastForward => Some("fast-forward".into()),
        B::Rewind => Some("rewind".into()),
        _ => None,
    }
}

async fn act(action: &str) {
    let request = match action {
        "fast-forward" => json!({"action":"seek","value":"15000000","key":"relative"}),
        "rewind" => json!({"action":"seek","value":"-15000000","key":"relative"}),
        other => json!({"action":other,"value":""}),
    };
    if let Err(error) = ipc::call(&request).await {
        log::warn!("SMTC: {error}");
    }
}

// The dedicated handler thread forwards button events into runtime tasks, so
// the COM event callback itself never blocks or needs a runtime context.
static BUTTON_QUEUE: std::sync::Mutex<Option<mpsc::Sender<String>>> = std::sync::Mutex::new(None);

#[allow(non_snake_case)]
unsafe extern "system" fn wndproc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        PostQuitMessage(0);
    }
    DefWindowProcW(window, message, wparam, lparam)
}

struct Session {
    smtc: SystemMediaTransportControls,
    last: Snapshot,
}

impl Session {
    /// Runs entirely on one thread: COM calls and the interop object stay
    /// with the thread that created them.
    fn new() -> windows::core::Result<Self> {
        unsafe {
            let apartment = CoInitializeEx(None, COINIT_MULTITHREADED);
            if apartment.0 != 0 && apartment.0 != 1 && apartment.0 != 0x80010106u32 as i32 {
                // S_FALSE and RPC_E_CHANGED_MODE are harmless: the thread may
                // already hold an apartment.
                return Err(windows::core::Error::from_hresult(apartment));
            }
            let class = HSTRING::from("MusicZeroSmtc");
            let name = PCWSTR::from_raw(class.as_ptr());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                lpszClassName: name,
                style: WNDCLASS_STYLES(0),
                ..Default::default()
            };
            RegisterClassW(&wc);
            // A zero-sized hidden control window: the interop interface only
            // needs a native handle, nothing is drawn here.
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                name,
                PCWSTR::from_raw(HSTRING::from("MusicZero").as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )?;
            let interop: ISystemMediaTransportControlsInterop =
                factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
            let smtc = interop.GetForWindow::<SystemMediaTransportControls>(window)?;
            smtc.SetIsPlayEnabled(true)?;
            smtc.SetIsPauseEnabled(true)?;
            smtc.SetIsNextEnabled(true)?;
            smtc.SetIsStopEnabled(true)?;
            smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped)?;
            let sender = BUTTON_QUEUE.lock().unwrap().as_ref().cloned().unwrap();
            smtc.ButtonPressed(&TypedEventHandler::new(
                move |_: windows::core::Ref<SystemMediaTransportControls>,
                      args: windows::core::Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        let button = args.Button()?;
                        if let Some(action) = button_to_action(button) {
                            let _ = sender.send(action);
                        }
                    }
                    Ok(())
                },
            ))?;
            Ok(Self {
                smtc,
                last: Snapshot::default(),
            })
        }
    }

    fn play_status(&self) -> MediaPlaybackStatus {
        match self.last.status.as_str() {
            "Playing" => MediaPlaybackStatus::Playing,
            "Paused" | "Loading" => MediaPlaybackStatus::Paused,
            _ if self.last.title.is_empty() => MediaPlaybackStatus::Stopped,
            _ => MediaPlaybackStatus::Playing,
        }
    }

    fn apply(&mut self, next: Snapshot) {
        let previous = std::mem::replace(&mut self.last, next.clone());
        let smtc = &self.smtc;
        {
            let _ = smtc.SetPlaybackStatus(self.play_status());
            let _ = smtc.SetIsNextEnabled(true);
            let _ = smtc.SetIsPauseEnabled(next.status == "Playing");
            let _ = smtc.SetIsPreviousEnabled(next.can_previous && !next.title.is_empty());
            // The OS interpolates the timeline between updates, so the taskbar
            // flyout tracks the position like any other Windows media player.
            if let Ok(timeline) = SystemMediaTransportControlsTimelineProperties::new() {
                let _ = timeline.SetStartTime(TimeSpan { Duration: 0 });
                let _ = timeline.SetPosition(TimeSpan {
                    Duration: next.position_ms.min(next.duration_ms) as i64 * 10_000,
                });
                let _ = timeline.SetMinSeekTime(TimeSpan { Duration: 0 });
                let _ = timeline.SetMaxSeekTime(TimeSpan {
                    Duration: next.duration_ms as i64 * 10_000,
                });
                let _ = timeline.SetEndTime(TimeSpan {
                    Duration: next.duration_ms as i64 * 10_000,
                });
                let _ = smtc.UpdateTimelineProperties(&timeline);
            }
            let _ = smtc.SetPlaybackRate(1.0f64);
            if next.title != previous.title
                || next.artist != previous.artist
                || next.art_url != previous.art_url
            {
                if let Ok(updater) = smtc.DisplayUpdater() {
                    // ClearAll also drops a stale artwork left from the
                    // previous track, so always rebuild the whole description.
                    let _ = updater.ClearAll();
                    let _ = updater.SetType(MediaPlaybackType::Music);
                    if let Ok(properties) = updater.MusicProperties() {
                        let _ = properties.SetTitle(&HSTRING::from(next.title.clone()));
                        let _ = properties.SetArtist(&HSTRING::from(format!(
                            "{} — {}",
                            next.artist, next.source
                        )));
                    }
                    if !next.art_url.is_empty() {
                        if let Ok(uri) = Uri::CreateUri(&HSTRING::from(next.art_url.clone())) {
                            if let Ok(thumbnail) = RandomAccessStreamReference::CreateFromUri(&uri)
                            {
                                let _ = updater.SetThumbnail(&thumbnail);
                            }
                        }
                    }
                    let _ = updater.Update();
                }
            }
        }
    }

    fn run(mut self, updates: mpsc::Receiver<Message>) {
        let mut message = MSG::default();
        loop {
            let mut drained = false;
            while let Ok(update) = updates.try_recv() {
                drained = true;
                match update {
                    Message::State(state) => self.apply(state),
                }
            }
            unsafe {
                if PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    drained = true;
                    let _ = GetMessageW(&mut message, None, 0, 0);
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                    if message.message == WM_QUIT {
                        return;
                    }
                }
            }
            if !drained {
                let wait = unsafe {
                    MsgWaitForMultipleObjectsEx(None, 500, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
                };
                if wait == WAIT_TIMEOUT {
                    continue;
                }
            }
        }
    }
}

pub fn spawn() {
    let (state_tx, state_rx) = mpsc::channel::<Message>();
    let (action_tx, action_rx) = mpsc::channel::<String>();
    *BUTTON_QUEUE.lock().unwrap() = Some(action_tx);
    let runtime = tokio::runtime::Handle::current();
    let _ = std::thread::Builder::new()
        .name("mz-smtc-actions".into())
        .spawn(move || {
            while let Ok(action) = action_rx.recv() {
                let _ = runtime.spawn(async move { act(&action).await });
            }
        });
    let _ = std::thread::Builder::new()
        .name("mz-smtc".into())
        .spawn(move || match Session::new() {
            Ok(session) => session.run(state_rx),
            Err(error) => log::warn!("SMTC: {error}"),
        });
    tokio::spawn(async move {
        let mut running = true;
        loop {
            match ipc::call(&json!({"action":"status"})).await {
                Ok(status) => {
                    if let Some(state) = snapshot(&status) {
                        running = state_tx.send(Message::State(state)).is_ok();
                    }
                }
                Err(error) => {
                    if running {
                        log::debug!("SMTC status: {error}");
                    }
                }
            }
            if !running {
                break;
            }
            tokio::time::sleep(POLL).await;
        }
    });
}
