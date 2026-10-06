use crate::ipc;
use crate::plugin::Module;
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Tray, TrayMethods};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct MusicTray {
    modules: Vec<(String, String)>,
    selected: usize,
    playlists: Vec<(String, String)>,
    current_playlist: usize,
    playing: String,
    can_previous: bool,
    history: Vec<(String, String)>,
}

fn send(action: &'static str, value: String) {
    tokio::spawn(async move {
        if let Err(error) = ipc::call(&json!({"action":action,"value":value})).await {
            log::warn!("Трей: {error}");
        }
    });
}

impl Tray for MusicTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "musiczero".into()
    }
    fn title(&self) -> String {
        "MusicZero".into()
    }
    fn icon_name(&self) -> String {
        "audio-x-generic".into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![
            StandardItem {
                label: self.playing.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
        ];
        for (label, action) in [
            ("Пауза / воспроизведение", "toggle"),
            ("Следующий трек", "next"),
        ] {
            items.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |_: &mut Self| send(action, String::new())),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(
            StandardItem {
                label: "Предыдущий трек".into(),
                enabled: self.can_previous,
                activate: Box::new(|_: &mut Self| send("previous", String::new())),
                ..Default::default()
            }
            .into(),
        );
        if !self.history.is_empty() {
            let module = self
                .modules
                .get(self.selected)
                .map(|(id, _)| id.clone())
                .unwrap_or_default();
            let submenu = self.history.iter().map(|(id, label)| {
                let id = id.clone();
                let module = module.clone();
                StandardItem {
                    label: label.clone(),
                    activate: Box::new(move |_: &mut Self| {
                        let id = id.clone();
                        let module = module.clone();
                        tokio::spawn(async move {
                            match ipc::call(&json!({"action":"replay","value":id,"key":"id","module":module})).await {
                                Ok(reply) if reply.get("error").is_some() => log::warn!("Трей: {}", reply["error"]),
                                Err(error) => log::warn!("Трей: {error}"),
                                _ => {}
                            }
                        });
                    }),
                    ..Default::default()
                }.into()
            }).collect();
            items.push(
                SubMenu {
                    label: "Последние пять треков".into(),
                    submenu,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        let ids: Vec<String> = self.modules.iter().map(|(id, _)| id.clone()).collect();
        let group: MenuItem<Self> = RadioGroup {
            selected: self.selected,
            select: Box::new(move |tray: &mut Self, index| {
                if let Some(id) = ids.get(index) {
                    tray.selected = index;
                    send("switch", id.clone());
                }
            }),
            options: self
                .modules
                .iter()
                .map(|(_, name)| RadioItem {
                    label: name.clone(),
                    ..Default::default()
                })
                .collect(),
        }
        .into();
        items.push(
            SubMenu {
                label: "Источник".into(),
                submenu: vec![group],
                ..Default::default()
            }
            .into(),
        );
        if !self.playlists.is_empty() {
            let ids: Vec<String> = self.playlists.iter().map(|(id, _)| id.clone()).collect();
            let group: MenuItem<Self> = RadioGroup {
                selected: self.current_playlist,
                select: Box::new(move |tray: &mut Self, index| {
                    if let Some(id) = ids.get(index) {
                        tray.current_playlist = index;
                        send("playlist", id.clone());
                    }
                }),
                options: self
                    .playlists
                    .iter()
                    .map(|(_, name)| RadioItem {
                        label: name.clone(),
                        ..Default::default()
                    })
                    .collect(),
            }
            .into();
            items.push(
                SubMenu {
                    label: "Плейлист".into(),
                    submenu: vec![group],
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Выход".into(),
                activate: Box::new(|_: &mut Self| send("quit", String::new())),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

pub fn spawn(modules: Vec<Module>) {
    tokio::spawn(async move {
        let tray = MusicTray {
            modules: modules
                .iter()
                .map(|m| (m.manifest.id.clone(), m.manifest.name.clone()))
                .collect(),
            selected: 0,
            playlists: Vec::new(),
            current_playlist: 0,
            playing: "Запуск MusicZero...".into(),
            can_previous: false,
            history: Vec::new(),
        };
        let mut warned = false;
        let handle = loop {
            match tray.clone().spawn().await {
                Ok(handle) => break Arc::new(handle),
                Err(error) => {
                    if !warned {
                        log::warn!("Трей: {error}; повторное подключение через 5 секунд");
                        warned = true;
                    }
                    // A user service can start before the desktop's tray watcher.
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        };
        let mut listed_for = (String::new(), String::new());
        let mut playlists = Vec::new();
        loop {
            let status = ipc::call(&json!({"action":"status"})).await;
            if let Ok(status) = status {
                let module = status["module"].as_str().unwrap_or("").to_owned();
                let title = status["title"].as_str().unwrap_or("");
                let artist = status["artist"].as_str().unwrap_or("");
                let playing = if title.is_empty() {
                    module.clone()
                } else {
                    format!("{artist} — {title}")
                };
                let playlist = status["playlist"].as_str().unwrap_or("").to_owned();
                if listed_for != (module.clone(), playlist.clone()) {
                    if let Ok(value) = ipc::call(&json!({"action":"playlists"})).await {
                        playlists = value["playlists"]
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
                        listed_for = (module.clone(), playlist.clone());
                    }
                }
                let playlists = playlists.clone();
                let can_previous = status["can_previous"].as_bool().unwrap_or(false);
                let history = status["history"]
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
                let _ = handle
                    .update(move |tray| {
                        tray.selected = tray
                            .modules
                            .iter()
                            .position(|(id, _)| *id == module)
                            .unwrap_or(0);
                        tray.current_playlist = playlists
                            .iter()
                            .position(|(id, _)| *id == playlist)
                            .unwrap_or(0);
                        tray.playlists = playlists;
                        tray.playing = playing;
                        tray.can_previous = can_previous;
                        tray.history = history;
                    })
                    .await;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}
