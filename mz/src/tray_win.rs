//! Windows tray icon and menu, mirroring the Linux ksni tray. Commands use
//! the same control channel as CLI clients. Status polling never blocks
//! playback and stays quiet when the host is unavailable.
use crate::config::{Language, Settings};
use crate::ipc;
use crate::plugin::Module;
use serde_json::{json, Value};
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use windows::core::{HSTRING, PCWSTR};

const CALLBACK: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;
const POLL: Duration = Duration::from_secs(2);

#[derive(Clone)]
enum Action {
    Toggle,
    Next,
    Previous,
    History(String, String),
    Switch(String),
    Playlist(String),
    Quit,
}

#[derive(Clone)]
struct TrayState {
    lang: Language,
    modules: Vec<(String, String)>,
    selected: usize,
    playlists: Vec<(String, String)>,
    playlist: usize,
    playing: String,
    can_previous: bool,
    history: Vec<(String, String)>,
    history_module: String,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

static STATE: Mutex<Option<TrayState>> = Mutex::new(None);
static QUEUE: Mutex<Option<mpsc::Sender<Action>>> = Mutex::new(None);

fn text(lang: Language, id: &str) -> &'static str {
    match id {
        "starting" => lang.text("Запуск MusicZero...", "Starting MusicZero..."),
        "toggle" => lang.text("Пауза / воспроизведение", "Pause / play"),
        "next" => lang.text("Следующий трек", "Next track"),
        "previous" => lang.text("Предыдущий трек", "Previous track"),
        "history" => lang.text("Последние пять треков", "Five recent tracks"),
        "source" => lang.text("Источник", "Provider"),
        "playlist" => lang.text("Плейлист", "Playlist"),
        "quit" => lang.text("Выход", "Quit"),
        _ => "",
    }
}

fn apply_state(status: &Value, state: &mut TrayState) {
    let module = status["module"].as_str().unwrap_or("").to_owned();
    let title = status["title"].as_str().unwrap_or("");
    let artist = status["artist"].as_str().unwrap_or("");
    let playing = if title.is_empty() {
        module.clone()
    } else {
        format!("{artist} — {title}")
    };
    let playlist = status["playlist"].as_str().unwrap_or("").to_owned();
    state.selected = state
        .modules
        .iter()
        .position(|(id, _)| *id == module)
        .unwrap_or(state.selected);
    state.playlist = state
        .playlists
        .iter()
        .position(|(id, _)| *id == playlist)
        .unwrap_or(state.playlist);
    if !playing.is_empty() {
        state.playing = playing;
    }
    state.can_previous = status["can_previous"].as_bool().unwrap_or(false);
    state.history = status["history"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|track| {
            Some((
                track["id"].as_str()?.to_owned(),
                format!(
                    "{} — {}",
                    track["artist"].as_str().unwrap_or(""),
                    track["title"].as_str().unwrap_or("")
                ),
            ))
        })
        .collect();
}

fn request(action: &Action) -> Value {
    match action {
        Action::Toggle => json!({"action":"toggle","value":""}),
        Action::Next => json!({"action":"next","value":""}),
        Action::Previous => json!({"action":"previous","value":""}),
        Action::History(module, id) => {
            json!({"action":"replay","value":id,"key":"id","module":module})
        }
        Action::Switch(id) => json!({"action":"switch","value":id}),
        Action::Playlist(id) => json!({"action":"playlist","value":id}),
        Action::Quit => json!({"action":"quit","value":""}),
    }
}

fn perform(action: &Action, runtime: tokio::runtime::Handle) {
    let quit = matches!(action, Action::Quit);
    let request = request(action);
    if let Err(error) = runtime.block_on(async { ipc::call(&request).await }) {
        log::warn!("Трей: {error}");
    }
    if quit {
        std::process::exit(0);
    }
}

/// Polls the host status exactly like the Linux tray and keeps the shared menu
/// state current; the Win32 window thread reads it on demand.
async fn poll() {
    let mut listed_for: (String, String) = (String::new(), String::new());
    loop {
        if let Ok(status) = ipc::call(&json!({"action":"status"})).await {
            let module = status["module"].as_str().unwrap_or("").to_owned();
            let playlist = status["playlist"].as_str().unwrap_or("").to_owned();
            if listed_for != (module.clone(), playlist.clone()) {
                if let Ok(value) = ipc::call(&json!({"action":"playlists"})).await {
                    let playlists = value["playlists"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|item| {
                            Some((
                                item["id"].as_str()?.to_owned(),
                                item["name"].as_str()?.to_owned(),
                            ))
                        })
                        .collect();
                    if let Some(state) = STATE.lock().unwrap().as_mut() {
                        state.playlists = playlists;
                        state.playlist = 0;
                        listed_for = (module.clone(), playlist.clone());
                    }
                }
            }
            if let Some(state) = STATE.lock().unwrap().as_mut() {
                apply_state(&status, state);
            }
        }
        tokio::time::sleep(POLL).await;
    }
}

pub(crate) mod native {
    use super::*;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, HGDIOBJ,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
        NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, GetCursorPos, GetMessageW, InsertMenuItemW, PostMessageW,
        PostQuitMessage, RegisterClassW, SetForegroundWindow, TrackPopupMenuEx, TranslateMessage,
        HICON, HMENU, ICONINFO, MENUITEMINFOW, MENU_ITEM_MASK, MENU_ITEM_STATE,
        MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU, MSG, MFS_CHECKED,
        MFS_ENABLED, MFS_GRAYED, MFT_SEPARATOR, MFT_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    };

    pub(crate) struct Anchor {
        icon: HICON,
    }

    /// Draws the play glyph programmatically; no icon assets are bundled with
    /// the player.
    pub(crate) fn icon() -> windows::core::Result<Anchor> {
        unsafe {
            let header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 32,
                    biHeight: -32,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels: *mut core::ffi::c_void = core::ptr::null_mut();
            let screen = CreateCompatibleDC(None);
            let bitmap = CreateDIBSection(Some(screen), &header, DIB_RGB_COLORS, &mut pixels, None, 0)?;
            let glyph = std::slice::from_raw_parts_mut(pixels.cast::<u8>(), 32 * 32 * 4);
            for y in 0..32u32 {
                for x in 0..32u32 {
                    let dx = x as f32 + 0.5 - 16.0;
                    let dy = y as f32 + 0.5 - 16.0;
                    if dx * dx + dy * dy > 15.2 * 15.2 {
                        continue;
                    }
                    let step = x as i32 - 11;
                    let inside = step >= 0
                        && step <= 15
                        && y as i32 >= 7 + step * 9 / 15
                        && y as i32 <= 25 - step * 9 / 15;
                    let (red, green, blue) = if inside {
                        (255u8, 255u8, 255u8)
                    } else {
                        (79u8, 70u8, 229u8)
                    };
                    let offset = ((y * 32 + x) * 4) as usize;
                    glyph[offset] = blue;
                    glyph[offset + 1] = green;
                    glyph[offset + 2] = red;
                    glyph[offset + 3] = 255;
                }
            }
            let _ = DeleteDC(screen);
            let mask_bits = [0u8; 128];
            let mask = CreateBitmap(
                32,
                32,
                1,
                1,
                Some(mask_bits.as_ptr().cast::<core::ffi::c_void>()),
            );
            let result = CreateIconIndirect(&ICONINFO {
                fIcon: windows::core::BOOL(1),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: bitmap,
            });
            let _ = DeleteObject(HGDIOBJ(mask.0));
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            Ok(Anchor { icon: result? })
        }
    }

    unsafe fn item(
        menu: HMENU,
        id: u32,
        sub: HMENU,
        state: MENU_ITEM_STATE,
        label: &str,
    ) {
        let mut label16 = wide(label);
        let mut info = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MENU_ITEM_MASK(MIIM_ID.0 | MIIM_STATE.0 | MIIM_FTYPE.0 | MIIM_STRING.0),
            fType: if label.is_empty() {
                MFT_SEPARATOR
            } else {
                MFT_STRING
            },
            fState: state,
            wID: id,
            dwTypeData: windows::core::PWSTR(label16.as_mut_ptr()),
            ..Default::default()
        };
        if !sub.is_invalid() {
            info.fMask = MENU_ITEM_MASK(info.fMask.0 | MIIM_SUBMENU.0);
            info.hSubMenu = sub;
        }
        let _ = InsertMenuItemW(menu, u32::MAX, true, &info);
    }

    unsafe fn add(window: HWND, icon: HICON) -> windows::core::Result<()> {
        let mut tip = [0u16; 128];
        for (slot, character) in tip.iter_mut().zip(wide("MusicZero")) {
            *slot = character;
        }
        let info = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE.0 | NIF_ICON.0 | NIF_TIP.0),
            uCallbackMessage: CALLBACK,
            hIcon: icon,
            szTip: tip,
            ..Default::default()
        };
        if !Shell_NotifyIconW(NIM_ADD, &info).as_bool() {
            return Err(windows::core::Error::from_thread());
        }
        Ok(())
    }

    unsafe fn choose(window: HWND, state: &TrayState) -> Option<Action> {
        let mut commands: Vec<(u32, Action)> = Vec::new();
        let mut next = 1u32;
        /// Registers the action bound to the next free command id.
        fn take(
            commands: &mut Vec<(u32, Action)>,
            next: &mut u32,
            action: Option<Action>,
        ) -> u32 {
            let id = *next;
            *next += 1;
            if let Some(action) = action {
                commands.push((id, action));
            }
            id
        }
        let menu = CreatePopupMenu().ok()?;
        let lang = state.lang;
        item(menu, 0, HMENU::default(), MFS_GRAYED, &state.playing);
        item(menu, 0, HMENU::default(), MFS_ENABLED, "");
        item(
            menu,
            take(&mut commands, &mut next, Some(Action::Toggle)),
            HMENU::default(),
            MFS_ENABLED,
            text(lang, "toggle"),
        );
        item(
            menu,
            take(&mut commands, &mut next, Some(Action::Next)),
            HMENU::default(),
            MFS_ENABLED,
            text(lang, "next"),
        );
        item(
            menu,
            take(&mut commands, &mut next, Some(Action::Previous)),
            HMENU::default(),
            if state.can_previous { MFS_ENABLED } else { MFS_GRAYED },
            text(lang, "previous"),
        );
        if !state.history.is_empty() {
            let history = CreatePopupMenu().ok()?;
            for (id, label) in state.history.clone() {
                let action = Action::History(state.history_module.clone(), id);
                let entry = take(&mut commands, &mut next, Some(action));
                item(history, entry, HMENU::default(), MFS_ENABLED, &label);
            }
            item(menu, 0, history, MFS_ENABLED, text(lang, "history"));
        }
        let source = CreatePopupMenu().ok()?;
        for (index, (id, name)) in state.modules.iter().enumerate() {
            let entry = take(&mut commands, &mut next, Some(Action::Switch(id.clone())));
            item(
                source,
                entry,
                HMENU::default(),
                if index == state.selected { MFS_CHECKED } else { MFS_ENABLED },
                name,
            );
        }
        item(menu, 0, source, MFS_ENABLED, text(lang, "source"));
        if !state.playlists.is_empty() {
            let playlists = CreatePopupMenu().ok()?;
            for (index, (id, name)) in state.playlists.iter().enumerate() {
                let entry = take(&mut commands, &mut next, Some(Action::Playlist(id.clone())));
                item(
                    playlists,
                    entry,
                    HMENU::default(),
                    if index == state.playlist {
                        MFS_CHECKED
                    } else {
                        MFS_ENABLED
                    },
                    name,
                );
            }
            item(menu, 0, playlists, MFS_ENABLED, text(lang, "playlist"));
        }
        item(
            menu,
            take(&mut commands, &mut next, Some(Action::Quit)),
            HMENU::default(),
            MFS_ENABLED,
            text(lang, "quit"),
        );
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(
            menu,
            TPM_LEFTALIGN.0 | TPM_RIGHTBUTTON.0 | TPM_BOTTOMALIGN.0 | TPM_RETURNCMD.0,
            point.x,
            point.y,
            window,
            None,
        )
        .0;
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        let chosen = chosen as u32;
        commands
            .into_iter()
            .find_map(|(id, action)| (id == chosen).then_some(action))
    }

    unsafe extern "system" fn wndproc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == CALLBACK {
            match lparam.0 as u32 {
                WM_LBUTTONUP | WM_RBUTTONUP => {
                    if let Some(state) = STATE.lock().unwrap().as_ref().cloned() {
                        if let Some(action) = choose(window, &state) {
                            if let Some(queue) = QUEUE.lock().unwrap().as_ref().cloned() {
                                let _ = queue.send(action);
                            }
                        }
                    }
                    return LRESULT(0);
                }
                WM_LBUTTONDBLCLK => {
                    if let Some(queue) = QUEUE.lock().unwrap().as_ref().cloned() {
                        let _ = queue.send(Action::Toggle);
                    }
                    return LRESULT(0);
                }
                _ => {}
            }
        }
        if message == WM_DESTROY {
            let info = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &info);
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }

    pub(crate) fn run(anchor: Anchor) {        unsafe {
            let class = HSTRING::from("MusicZeroTray");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                lpszClassName: PCWSTR::from_raw(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let name = PCWSTR::from_raw(class.as_ptr());
            let window = match CreateWindowExW(
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
            ) {
                Ok(window) => window,
                Err(error) => {
                    log::warn!("Трей: {error}");
                    return;
                }
            };
            if let Err(error) = add(window, anchor.icon) {
                log::warn!("Трей: {error}");
            }
            let mut message = MSG::default();
            loop {
                if GetMessageW(&mut message, None, 0, 0).0 == 0 {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

pub fn spawn(modules: Vec<Module>) {
    let settings = Settings::load().ok();
    let lang = settings.map(|value| value.language).unwrap_or_default();
    let state = TrayState {
        lang,
        modules: modules
            .iter()
            .map(|module| (module.manifest.id.clone(), module.manifest.name.clone()))
            .collect(),
        selected: 0,
        playlists: Vec::new(),
        playlist: 0,
        playing: text(lang, "starting").to_string(),
        can_previous: false,
        history: Vec::new(),
        history_module: String::new(),
    };
    *STATE.lock().unwrap() = Some(state);
    let (sender, receiver) = mpsc::channel::<Action>();
    *QUEUE.lock().unwrap() = Some(sender);
    let runtime = tokio::runtime::Handle::current();
    let _ = std::thread::Builder::new()
        .name("mz-tray-actions".into())
        .spawn(move || {
            while let Ok(action) = receiver.recv() {
                perform(&action, runtime.clone());
            }
        });
    let _ = std::thread::Builder::new()
        .name("mz-tray".into())
        .spawn(move || match native::icon() {
            Ok(anchor) => native::run(anchor),
            Err(error) => log::warn!("Трей: {error}"),
        });
    tokio::spawn(poll());
}