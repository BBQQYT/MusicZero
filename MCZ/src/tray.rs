//! Shared StatusNotifierItem menu for both music providers.
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Tray, TrayMethods};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct TrayConfig {
    pub id: &'static str,
    pub title: &'static str,
    pub service: &'static str,
    pub wave_settings: bool,
}

struct MusicTray {
    config: TrayConfig,
    conn: zbus::Connection,
    playlists: Vec<(String, String)>,
    current: usize,
    playing: String,
    available: bool,
    wave: (String, String, String),
}

async fn control<'a>(conn: &'a zbus::Connection, cfg: TrayConfig) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(conn, cfg.service, "/org/mcz/Control", "org.mcz.Control").await
}

async fn player<'a>(conn: &'a zbus::Connection, cfg: TrayConfig) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(
        conn,
        cfg.service,
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

fn options(
    label: &str,
    selected: &str,
    values: &[(&str, &str)],
    key: &'static str,
) -> MenuItem<MusicTray> {
    let index = values.iter().position(|(v, _)| *v == selected).unwrap_or(0);
    let ids: Vec<String> = values.iter().map(|(id, _)| (*id).to_owned()).collect();
    let group: MenuItem<MusicTray> = RadioGroup {
        selected: index,
        select: Box::new(move |tray: &mut MusicTray, idx| {
            let Some(value) = ids.get(idx).cloned() else {
                return;
            };
            let conn = tray.conn.clone();
            let cfg = tray.config;
            fire(async move {
                let proxy = control(&conn, cfg).await?;
                let _: () = proxy.call("SetWaveSetting", &(key, value.as_str())).await?;
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

impl Tray for MusicTray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        self.config.id.into()
    }
    fn title(&self) -> String {
        self.config.title.into()
    }
    fn icon_name(&self) -> String {
        "audio-x-generic".into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.config.title.into(),
            description: if self.available {
                self.playing.clone()
            } else {
                "Демон недоступен".into()
            },
            icon_name: String::new(),
            icon_pixmap: vec![],
        }
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        if !self.available {
            return vec![StandardItem {
                label: "Демон недоступен".into(),
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
        let ids: Vec<String> = self.playlists.iter().map(|(id, _)| id.clone()).collect();
        let selected_name = self
            .playlists
            .get(self.current)
            .map(|(_, n)| n.as_str())
            .unwrap_or("—");
        let playlist_group: MenuItem<Self> = RadioGroup {
            selected: self.current,
            select: Box::new(move |tray: &mut Self, idx| {
                let Some(id) = ids.get(idx).cloned() else {
                    return;
                };
                tray.current = idx;
                let conn = tray.conn.clone();
                let cfg = tray.config;
                fire(async move {
                    let proxy = control(&conn, cfg).await?;
                    let _: () = proxy.call("SetPlaylist", &(id.as_str(),)).await?;
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
        if self.config.wave_settings
            && self
                .playlists
                .get(self.current)
                .is_some_and(|(id, _)| id == "wave")
        {
            menu.push(MenuItem::Separator);
            let mood = options(
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
            let diversity = options(
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
            let language = options(
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
        menu.push(MenuItem::Separator);
        for (label, method) in [
            ("Следующий трек", "Next"),
            ("Пауза / Воспроизведение", "PlayPause"),
        ] {
            menu.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| {
                        let conn = tray.conn.clone();
                        let cfg = tray.config;
                        fire(async move {
                            let proxy = player(&conn, cfg).await?;
                            let _: () = proxy.call(method, &()).await?;
                            Ok(())
                        });
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        menu
    }
}

pub async fn run(config: TrayConfig) -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;
    let handle = Arc::new(
        MusicTray {
            config,
            conn: conn.clone(),
            playlists: vec![],
            current: 0,
            playing: "Ничего не играет".into(),
            available: false,
            wave: ("all".into(), "default".into(), "any".into()),
        }
        .spawn()
        .await?,
    );
    let mut cached_playlists = Vec::new();
    let mut last_refresh = Instant::now() - Duration::from_secs(31);
    loop {
        if let Ok(proxy) = control(&conn, config).await {
            if cached_playlists.is_empty() || last_refresh.elapsed() >= Duration::from_secs(30) {
                if let Ok(list) = proxy.call("ListPlaylists", &()).await {
                    cached_playlists = list;
                    last_refresh = Instant::now();
                }
            }
            let current: zbus::Result<String> = proxy.call("CurrentPlaylist", &()).await;
            let playing: zbus::Result<(String, String)> = proxy.call("NowPlaying", &()).await;
            let wave: Option<(String, String, String)> = if config.wave_settings {
                proxy.call("WaveSettings", &()).await.ok()
            } else {
                None
            };
            if let (Ok(current), Ok((artist, title))) = (current, playing) {
                let playlists = cached_playlists.clone();
                let index = playlists
                    .iter()
                    .position(|(id, _)| id == &current)
                    .unwrap_or(0);
                let label = if title.is_empty() {
                    "Ничего не играет".into()
                } else {
                    format!("{artist} — {title}")
                };
                let _ = handle
                    .update(move |t| {
                        t.playlists = playlists;
                        t.current = index;
                        t.playing = label;
                        if let Some(wave) = wave {
                            t.wave = wave;
                        }
                        t.available = true;
                    })
                    .await;
            }
        } else {
            cached_playlists.clear();
            let _ = handle.update(|t| t.available = false).await;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
