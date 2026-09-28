//! StatusNotifierItem menu for MusicZero providers (single unified tray).
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Tray, TrayMethods};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zbus::names::BusName;

#[derive(Clone, Copy)]
pub struct TrayConfig {
    pub id: &'static str,
    pub title: &'static str,
    pub service: &'static str,
    pub wave_settings: bool,
}

const YMZ_BUS: &str = "org.mpris.MediaPlayer2.ymz";
const YOUMZ_BUS: &str = "org.mpris.MediaPlayer2.youmz";

async fn control_for<'a>(conn: &'a zbus::Connection, service: &'a str) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(conn, service, "/org/mcz/Control", "org.mcz.Control").await
}

async fn player_for<'a>(conn: &'a zbus::Connection, service: &'a str) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(
        conn,
        service,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await
}

fn fire(f: impl std::future::Future<Output = zbus::Result<()>> + Send + 'static) {
    tokio::spawn(async move {
        if let Err(e) = f.await {
            log::error!("Команда трея: {e}");
        }
    });
}

fn unified_options(
    label: &str,
    selected: &str,
    values: &[(&str, &str)],
    key: &'static str,
) -> MenuItem<UnifiedTray> {
    let index = values.iter().position(|(v, _)| *v == selected).unwrap_or(0);
    let ids: Vec<String> = values.iter().map(|(id, _)| (*id).to_owned()).collect();
    let group: MenuItem<UnifiedTray> = RadioGroup {
        selected: index,
        select: Box::new(move |tray: &mut UnifiedTray, idx| {
            let Some(value) = ids.get(idx).cloned() else {
                return;
            };
            let conn = tray.conn.clone();
            let bus = tray.service_bus_name();
            fire(async move {
                let proxy = control_for(&conn, bus).await?;
                let () = proxy.call("SetWaveSetting", &(key, value.as_str())).await?;
                Ok(())
            });
        }),
        options: values
            .iter()
            .map(|(_, text)| RadioItem {
                label: (*text).into(),
                ..Default::default()
            })
            .collect(),
    }
    .into();
    SubMenu {
        label: label.into(),
        submenu: vec![group],
        ..Default::default()
    }
    .into()
}

pub struct UnifiedTray {
    conn: zbus::Connection,
    active_service: String,
    ymz_available: bool,
    youmz_available: bool,
    playlists: Vec<(String, String)>,
    current_playlist: usize,
    playing: String,
    available: bool,
    wave: (String, String, String),
}

impl UnifiedTray {
    fn service_bus_name(&self) -> &'static str {
        if self.active_service == "youmz" {
            YOUMZ_BUS
        } else {
            YMZ_BUS
        }
    }
}

impl Tray for UnifiedTray {
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

    fn tool_tip(&self) -> ksni::ToolTip {
        let label = if self.active_service == "youmz" {
            "YouTube Music"
        } else {
            "Яндекс Музыка"
        };
        ksni::ToolTip {
            title: format!("MusicZero ({label})"),
            description: if self.available {
                self.playing.clone()
            } else {
                "Плееры недоступны".into()
            },
            icon_name: String::new(),
            icon_pixmap: vec![],
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        if !self.available {
            return vec![StandardItem {
                label: "Плееры не запущены".into(),
                enabled: false,
                ..Default::default()
            }
            .into()];
        }

        let mut menu = vec![
            StandardItem {
                label: self.playing.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
        ];

        // Кнопки управления активным сервисом
        for (label, method) in [
            ("Пауза / Воспроизведение", "PlayPause"),
            ("Следующий трек", "Next"),
            ("Предыдущий трек", "Previous"),
        ] {
            let bus = self.service_bus_name();
            menu.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| {
                        let conn = tray.conn.clone();
                        fire(async move {
                            let proxy = player_for(&conn, bus).await?;
                            let () = proxy.call(method, &()).await?;
                            Ok(())
                        });
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        // Если запущены оба сервиса — переключатель источника в трее
        if self.ymz_available && self.youmz_available {
            menu.push(MenuItem::Separator);
            let selected_idx = if self.active_service == "youmz" { 1 } else { 0 };
            let service_group: MenuItem<Self> = RadioGroup {
                selected: selected_idx,
                select: Box::new(move |tray: &mut Self, idx| {
                    let new_service = if idx == 1 { "youmz" } else { "ymz" };
                    if tray.active_service == new_service {
                        return;
                    }
                    let old_bus = tray.service_bus_name();
                    tray.active_service = new_service.to_string();
                    let new_bus = tray.service_bus_name();
                    let conn = tray.conn.clone();
                    fire(async move {
                        if let Ok(p_old) = player_for(&conn, old_bus).await {
                            let _: Result<(), _> = p_old.call("Pause", &()).await;
                        }
                        if let Ok(p_new) = player_for(&conn, new_bus).await {
                            let _: Result<(), _> = p_new.call("Play", &()).await;
                        }
                        Ok(())
                    });
                }),
                options: vec![
                    RadioItem {
                        label: "Яндекс Музыка (YMZ)".into(),
                        ..Default::default()
                    },
                    RadioItem {
                        label: "YouTube Music (YouMZ)".into(),
                        ..Default::default()
                    },
                ],
            }
            .into();

            menu.push(
                SubMenu {
                    label: "Источник звука".into(),
                    submenu: vec![service_group],
                    ..Default::default()
                }
                .into(),
            );
        }

        // Подменю плейлистов текущего сервиса
        if !self.playlists.is_empty() {
            menu.push(MenuItem::Separator);
            let ids: Vec<String> = self.playlists.iter().map(|(id, _)| id.clone()).collect();
            let selected_name = self
                .playlists
                .get(self.current_playlist)
                .map(|(_, n)| n.as_str())
                .unwrap_or("—");
            let bus = self.service_bus_name();
            let playlist_group: MenuItem<Self> = RadioGroup {
                selected: self.current_playlist,
                select: Box::new(move |tray: &mut Self, idx| {
                    let Some(id) = ids.get(idx).cloned() else {
                        return;
                    };
                    tray.current_playlist = idx;
                    let conn = tray.conn.clone();
                    fire(async move {
                        let proxy = control_for(&conn, bus).await?;
                        let () = proxy.call("SetPlaylist", &(id.as_str(),)).await?;
                        Ok(())
                    });
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

            menu.push(
                SubMenu {
                    label: format!("Плейлист: {selected_name}"),
                    submenu: vec![playlist_group],
                    ..Default::default()
                }
                .into(),
            );
        }

        // Настройки волны (если выбран YMZ и играет «Моя волна»)
        if self.active_service == "ymz"
            && self
                .playlists
                .get(self.current_playlist)
                .is_some_and(|(id, _)| id == "wave")
        {
            menu.push(MenuItem::Separator);
            let mood = unified_options(
                "Настроение",
                &self.wave.0,
                &[
                    ("all", "Любое"),
                    ("fun", "Весёлое"),
                    ("active", "Бодрое"),
                    ("calm", "Спокойное"),
                    ("sad", "Грустное"),
                ],
                "mood",
            );
            let diversity = unified_options(
                "Разнообразие",
                &self.wave.1,
                &[
                    ("default", "Обычное"),
                    ("favorite", "Любимое"),
                    ("popular", "Популярное"),
                    ("discover", "Незнакомое"),
                ],
                "diversity",
            );
            let language = unified_options(
                "Язык",
                &self.wave.2,
                &[
                    ("any", "Любой"),
                    ("russian", "Русский"),
                    ("not-russian", "Иностранный"),
                ],
                "language",
            );
            menu.push(
                SubMenu {
                    label: "Моя волна".into(),
                    submenu: vec![mood, diversity, language],
                    ..Default::default()
                }
                .into(),
            );
        }

        menu
    }
}

pub async fn run_unified() -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;

    let tray = UnifiedTray {
        conn: conn.clone(),
        active_service: "ymz".into(),
        ymz_available: false,
        youmz_available: false,
        playlists: vec![],
        current_playlist: 0,
        playing: "Запуск MusicZero...".into(),
        available: false,
        wave: ("all".into(), "default".into(), "any".into()),
    };

    let handle = Arc::new(tray.spawn().await?);
    let mut cached_service = String::new();
    let mut cached_playlists: Vec<(String, String)> = Vec::new();
    let mut last_playlist_refresh = Instant::now() - Duration::from_secs(31);

    loop {
        let ymz_bus_name = BusName::try_from(YMZ_BUS).unwrap();
        let youmz_bus_name = BusName::try_from(YOUMZ_BUS).unwrap();

        let ymz_owner = dbus.name_has_owner(ymz_bus_name).await.unwrap_or(false);
        let youmz_owner = dbus.name_has_owner(youmz_bus_name).await.unwrap_or(false);

        let available = ymz_owner || youmz_owner;

        let mut current_active = String::new();
        let _ = handle
            .update(|t| {
                if !t.ymz_available && ymz_owner && !youmz_owner {
                    t.active_service = "ymz".into();
                } else if !t.youmz_available && youmz_owner && !ymz_owner {
                    t.active_service = "youmz".into();
                } else if t.active_service == "ymz" && !ymz_owner && youmz_owner {
                    t.active_service = "youmz".into();
                } else if t.active_service == "youmz" && !youmz_owner && ymz_owner {
                    t.active_service = "ymz".into();
                }
                t.ymz_available = ymz_owner;
                t.youmz_available = youmz_owner;
                t.available = available;
                current_active = t.active_service.clone();
            })
            .await;

        if ymz_owner && youmz_owner {
            let ymz_playing = if let Ok(p) = player_for(&conn, YMZ_BUS).await {
                p.get_property::<String>("PlaybackStatus").await.unwrap_or_default() == "Playing"
            } else {
                false
            };
            let youmz_playing = if let Ok(p) = player_for(&conn, YOUMZ_BUS).await {
                p.get_property::<String>("PlaybackStatus").await.unwrap_or_default() == "Playing"
            } else {
                false
            };

            if youmz_playing && !ymz_playing && current_active != "youmz" {
                let _ = handle
                    .update(|t| {
                        t.active_service = "youmz".into();
                    })
                    .await;
                current_active = "youmz".into();
            } else if ymz_playing && !youmz_playing && current_active != "ymz" {
                let _ = handle
                    .update(|t| {
                        t.active_service = "ymz".into();
                    })
                    .await;
                current_active = "ymz".into();
            }
        }

        let target_service = if current_active.is_empty() {
            if ymz_owner { "ymz".into() } else { "youmz".into() }
        } else {
            current_active
        };

        if available {
            let bus_name = if target_service == "youmz" {
                YOUMZ_BUS
            } else {
                YMZ_BUS
            };

            let service_label = if target_service == "youmz" {
                "YouTube Music"
            } else {
                "Яндекс Музыка"
            };

            if cached_service != target_service {
                cached_playlists.clear();
                cached_service = target_service.clone();
            }

            if let Ok(proxy) = control_for(&conn, bus_name).await {
                if cached_playlists.is_empty() || last_playlist_refresh.elapsed() >= Duration::from_secs(30) {
                    if let Ok(list) = proxy.call::<_, _, Vec<(String, String)>>("ListPlaylists", &()).await {
                        cached_playlists = list;
                        last_playlist_refresh = Instant::now();
                    }
                }

                let current_pl: String = proxy.call("CurrentPlaylist", &()).await.unwrap_or_default();
                let playing_pair: (String, String) = proxy.call("NowPlaying", &()).await.unwrap_or_default();
                let wave_set: Option<(String, String, String)> = if target_service == "ymz" {
                    proxy.call("WaveSettings", &()).await.ok()
                } else {
                    None
                };

                let p_proxy = player_for(&conn, bus_name).await;
                let pb_status: String = match p_proxy {
                    Ok(p) => p.get_property("PlaybackStatus").await.unwrap_or_else(|_| "Stopped".into()),
                    Err(_) => "Stopped".into(),
                };

                let playlists = cached_playlists.clone();
                let index = playlists.iter().position(|(id, _)| id == &current_pl).unwrap_or(0);

                let status_icon = if pb_status == "Playing" {
                    "▶"
                } else if pb_status == "Paused" {
                    "⏸"
                } else {
                    "⏹"
                };

                let label = if playing_pair.1.is_empty() {
                    format!("{status_icon} Ничего не играет [{service_label}]")
                } else {
                    format!("{status_icon} {} — {} [{service_label}]", playing_pair.0, playing_pair.1)
                };

                let _ = handle
                    .update(move |t| {
                        t.playlists = playlists;
                        t.current_playlist = index;
                        t.playing = label;
                        if let Some(w) = wave_set {
                            t.wave = w;
                        }
                        t.available = true;
                    })
                    .await;
            }
        } else {
            cached_playlists.clear();
            let _ = handle
                .update(|t| {
                    t.available = false;
                    t.playing = "Плееры не запущены".into();
                })
                .await;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn run(_config: TrayConfig) -> Result<(), Box<dyn std::error::Error>> {
    run_unified().await
}
