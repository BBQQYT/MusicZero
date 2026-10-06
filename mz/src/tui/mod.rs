mod fields;
mod terminal;
use crate::{
    config::{Language, Settings},
    ipc,
    plugin::{self, Module},
    Result,
};
use serde_json::{json, Value};
use terminal::{Row, Ui};

fn checked(value: Value) -> Result<Value> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_owned().into());
    }
    Ok(value)
}
async fn active(module: &Module) -> bool {
    ipc::call(&json!({"action":"status"}))
        .await
        .ok()
        .is_some_and(|v| v["module"] == module.manifest.id)
}
async fn set(module: &Module, key: &str, value: &str) -> Result<Value> {
    if active(module).await {
        checked(ipc::call(&json!({"action":"set-setting","module":module.manifest.id,"key":key,"value":value})).await?)
    } else {
        module.config_json("set-setting", &[key, value]).await
    }
}
async fn playlist(module: &Module, id: &str) -> Result<()> {
    if active(module).await {
        checked(
            ipc::call(&json!({"action":"playlist","module":module.manifest.id,"value":id})).await?,
        )?;
    } else {
        plugin::save_playlist(&module.manifest.id, id)?;
    }
    Ok(())
}
async fn error(ui: &mut Ui, result: Result<()>) -> Result<()> {
    if let Err(error) = result {
        ui.message(ui.tr("Ошибка", "Error"), &error.to_string())
            .await?;
    }
    Ok(())
}
pub async fn run(initial: Option<&str>) -> Result<()> {
    let mut settings = Settings::load()?;
    let mut ui = Ui::open(settings.language)?;
    if let Some(id) = initial {
        let result = match id {
            "--player" => host_menu(&mut ui, &mut settings).await,
            "--service" => service_menu(&mut ui, &mut settings).await,
            _ => {
                async {
                    let modules = plugin::discover()?;
                    module_menu(&mut ui, crate::module(&modules, id)?).await
                }
                .await
            }
        };
        error(&mut ui, result).await?;
    }
    let mut selected = 0;
    loop {
        let discovered = plugin::discover();
        let discovery_error = discovered.as_ref().err().map(ToString::to_string);
        let modules = discovered.unwrap_or_default();
        let mut rows = vec![
            Row::new(
                format!(
                    "{}: {}",
                    ui.tr("Язык / Language", "Language / Язык"),
                    ui.tr("Русский", "English")
                ),
                ui.tr(
                    "Язык меню сохраняется и применяется сразу.",
                    "Menu language is saved and applied immediately.",
                ),
            ),
            Row::new(
                ui.tr("Плеер и папки", "Player and folders"),
                ui.tr(
                    "Трей, папки модулей и временных файлов, журналирование.",
                    "Tray, module and temporary folders, logging.",
                ),
            ),
        ];
        rows.extend(modules.iter().map(|module| {
            Row::new(
                module_name(ui.lang, module),
                ui.tr(
                    "Все настройки модуля, плейлисты и вход.",
                    "All provider settings, playlists and login.",
                ),
            )
        }));
        let service_index = crate::service::supported().then_some(rows.len());
        if service_index.is_some() {
            rows.push(Row::new(
                ui.tr("Сервис / автозапуск", "Service / autostart"),
                ui.tr(
                    "Установить пользовательский сервис, выбрать источник и управлять запуском.",
                    "Install a user service, choose its source and control startup.",
                ),
            ));
        }
        let refresh_index = rows.len();
        rows.push(Row::new(
            ui.tr("Обновить список модулей", "Refresh modules"),
            discovery_error.unwrap_or_else(|| {
                if modules.is_empty() {
                    ui.tr(
                        "Модули не найдены. Проверьте папку модулей в настройках плеера.",
                        "No modules found. Check the modules folder in player settings.",
                    )
                    .into()
                } else {
                    String::new()
                }
            }),
        ));
        rows.push(Row::new(ui.tr("Выход", "Exit"), ""));
        let Some(index) = ui
            .choose(ui.tr("Настройки", "Settings"), &rows, &mut selected)
            .await?
        else {
            break;
        };
        let result = match index {
            0 => language(&mut ui, &mut settings).await,
            1 => host_menu(&mut ui, &mut settings).await,
            i if i < modules.len() + 2 => module_menu(&mut ui, &modules[i - 2]).await,
            i if Some(i) == service_index => service_menu(&mut ui, &mut settings).await,
            i if i == refresh_index => Ok(()),
            _ => break,
        };
        error(&mut ui, result).await?;
    }
    Ok(())
}
fn module_name(lang: Language, module: &Module) -> String {
    match module.manifest.id.as_str() {
        "local" => lang.text("Локальная музыка", "Local music").into(),
        "ymz" => lang.text("Яндекс Музыка", "Yandex Music").into(),
        _ => module.manifest.name.clone(),
    }
}
async fn language(ui: &mut Ui, settings: &mut Settings) -> Result<()> {
    let rows = [Row::new("Русский", ""), Row::new("English", "")];
    let mut selected = usize::from(ui.lang == Language::En);
    if let Some(index) = ui.choose("Язык / Language", &rows, &mut selected).await? {
        let mut updated = settings.clone();
        updated.language = if index == 0 {
            Language::Ru
        } else {
            Language::En
        };
        updated.save()?;
        ui.lang = updated.language;
        *settings = updated;
    }
    Ok(())
}
async fn host_menu(ui: &mut Ui, settings: &mut Settings) -> Result<()> {
    let mut keys = vec!["modules_dir", "log_filter", "temp_dir"];
    if cfg!(target_os = "android") {
        keys.push("notifications_enabled");
    }
    if cfg!(all(feature = "tray", target_os = "linux")) {
        keys.push("tray_enabled");
    }
    let mut selected = 0;
    loop {
        let value = serde_json::to_value(&*settings)?;
        let mut rows: Vec<_> = keys
            .iter()
            .map(|key| {
                Row::new(
                    format!(
                        "{}: {}",
                        fields::label(ui.lang, key),
                        fields::display(ui.lang, "mz", key, &value[key])
                    ),
                    fields::hint(ui.lang, key),
                )
            })
            .collect();
        rows.push(Row::new(
            ui.tr("Текущие пути и окружение", "Current paths and environment"),
            "",
        ));
        let Some(index) = ui
            .choose(ui.tr("Плеер", "Player"), &rows, &mut selected)
            .await?
        else {
            return Ok(());
        };
        let result = async {
            if index == keys.len() {
                let paths = format!(
                    "Config: {}\nCache: {}\nModules: {}\n\n{}",
                    mcz::paths::config_dir("mz").display(),
                    mcz::paths::cache_dir("mz").display(),
                    plugin::module_dir()?.display(),
                    [
                        "MZ_MODULES_DIR",
                        "RUST_LOG",
                        "TMPDIR",
                        "TMP",
                        "TEMP",
                        "XDG_CONFIG_HOME",
                        "XDG_CACHE_HOME",
                        "XDG_RUNTIME_DIR",
                        "APPDATA",
                        "LOCALAPPDATA",
                        "YOUMZ_BROWSER",
                        "YOUMZ_PROXY",
                        "YM_TOKEN",
                        "YOUMZ_SESSION"
                    ]
                    .iter()
                    .filter_map(|key| std::env::var(key).ok().map(|value| format!(
                        "{key}: {}",
                        if ["YM_TOKEN", "YOUMZ_SESSION", "YOUMZ_PROXY"].contains(key) {
                            "••••••".into()
                        } else {
                            value
                        }
                    )))
                    .collect::<Vec<_>>()
                    .join("\n")
                );
                return ui
                    .message(ui.tr("Пути и окружение", "Paths and environment"), &paths)
                    .await;
            }
            let key = keys[index];
            if let Some(value) = fields::edit(ui, "mz", key, &value[key]).await? {
                let mut updated = settings.clone();
                updated.set(key, &value)?;
                updated.save()?;
                *settings = updated;
            }
            Ok(())
        }
        .await;
        error(ui, result).await?;
    }
}

async fn service_menu(ui: &mut Ui, settings: &mut Settings) -> Result<()> {
    if !crate::service::supported() {
        return Err("Сервис доступен в Linux с systemd --user".into());
    }
    let mut selected = 0;
    loop {
        let state = match crate::service::status().await {
            Ok(value) => {
                if !value["installed"].as_bool().unwrap_or(false) {
                    ui.tr("не установлен", "not installed").to_owned()
                } else {
                    format!(
                        "{}; {}",
                        if value["active"].as_bool().unwrap_or(false) {
                            ui.tr("работает", "running")
                        } else {
                            ui.tr("остановлен", "stopped")
                        },
                        if value["enabled"].as_bool().unwrap_or(false) {
                            ui.tr("автозапуск включён", "autostart enabled")
                        } else {
                            ui.tr("автозапуск выключен", "autostart disabled")
                        }
                    )
                }
            }
            Err(error) => error.to_string(),
        };
        let rows = [
            Row::new(ui.tr("Установить сервис", "Install service"), ui.tr("Выберите модуль для автозапуска при входе в систему. Без sudo; запуск сейчас — отдельно.", "Choose a provider to start at login. No sudo; starting now is optional.")),
            Row::new(ui.tr("Запустить сервис", "Start service"), ""),
            Row::new(ui.tr("Остановить сервис", "Stop service"), ""),
            Row::new(ui.tr("Перезапустить сервис", "Restart service"), ui.tr("Применить настройки трея и источник сервиса.", "Apply tray preferences and the service source.")),
            Row::new(ui.tr("Удалить сервис", "Remove service"), ui.tr("Остановить и отключить автозапуск. Настройки музыки сохраняются.", "Stop and disable autostart. Music settings are retained.")),
            Row::new(format!("{}: {state}", ui.tr("Статус", "Status")), format!("{}: {}", ui.tr("Источник", "Source"), settings.service_module)),
        ];
        let Some(index) = ui
            .choose(
                ui.tr("Сервис / автозапуск", "Service / autostart"),
                &rows,
                &mut selected,
            )
            .await?
        else {
            return Ok(());
        };
        let result = async {
            match index {
                0 => {
                    let modules = plugin::discover()?;
                    if modules.is_empty() {
                        return Err("Модули не найдены".into());
                    }
                    let rows: Vec<_> = modules
                        .iter()
                        .map(|module| Row::new(module_name(ui.lang, module), &module.manifest.id))
                        .collect();
                    let mut selected = modules
                        .iter()
                        .position(|module| module.manifest.id == settings.service_module)
                        .unwrap_or(0);
                    let Some(index) = ui
                        .choose(
                            ui.tr("Источник для сервиса", "Service source"),
                            &rows,
                            &mut selected,
                        )
                        .await?
                    else {
                        return Ok(());
                    };
                    ui.wait(crate::service::install(&modules[index], settings))
                        .await?;
                    let rows = [
                        Row::new(ui.tr("Позже", "Later"), ""),
                        Row::new(ui.tr("Запустить сейчас", "Start now"), ""),
                    ];
                    if ui
                        .choose(
                            ui.tr(
                                "Сервис установлен; автозапуск включён",
                                "Service installed; autostart enabled",
                            ),
                            &rows,
                            &mut 0,
                        )
                        .await?
                        == Some(1)
                    {
                        ui.wait(crate::service::action("start")).await?;
                    }
                }
                1..=3 => {
                    ui.wait(crate::service::action(
                        ["start", "stop", "restart"][index - 1],
                    ))
                    .await?;
                }
                4 => {
                    let rows = [
                        Row::new(ui.tr("Отмена", "Cancel"), ""),
                        Row::new(ui.tr("Удалить", "Remove"), ""),
                    ];
                    if ui
                        .choose(
                            ui.tr(
                                "Удалить сервис и отключить автозапуск?",
                                "Remove service and disable autostart?",
                            ),
                            &rows,
                            &mut 0,
                        )
                        .await?
                        == Some(1)
                    {
                        ui.wait(crate::service::action("remove")).await?;
                    }
                }
                _ => {
                    let value = ui.wait(crate::service::status()).await?;
                    ui.message(
                        ui.tr("Статус сервиса", "Service status"),
                        &serde_json::to_string_pretty(&value)?,
                    )
                    .await?;
                }
            }
            Ok(())
        }
        .await;
        error(ui, result).await?;
    }
}
async fn module_menu(ui: &mut Ui, module: &Module) -> Result<()> {
    ui.wait(module.validate()).await?;
    let mut response = match ui.wait(module.config_json("settings", &[])).await {
        Ok(value) => value,
        Err(err) => json!({"settings":{},"warning":err.to_string()}),
    };
    let mut selected = 0;
    loop {
        let settings = response["settings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let keys: Vec<_> = settings.keys().cloned().collect();
        let mut rows = vec![Row::new(
            format!(
                "{}: {}",
                ui.tr("Плейлист", "Playlist"),
                plugin::selected_playlist(module)
            ),
            ui.tr(
                "Выберите из списка; сохраняется и без запущенного плеера.",
                "Choose from a list; saved even while the player is stopped.",
            ),
        )];
        rows.extend(keys.iter().map(|key| {
            Row::new(
                format!(
                    "{}: {}",
                    fields::label(ui.lang, key),
                    fields::display(ui.lang, &module.manifest.id, key, &settings[key])
                ),
                fields::hint(ui.lang, key),
            )
        }));
        let login_index = rows.len();
        let can_login = !matches!(module.manifest.id.as_str(), "local" | "icecast");
        if can_login {
            rows.push(Row::new(
                ui.tr(
                    "Войти / обновить авторизацию",
                    "Log in / renew authorization",
                ),
                ui.tr(
                    "Откроет процедуру входа выбранного сервиса.",
                    "Open the provider's interactive login.",
                ),
            ));
        }
        let refresh_index = rows.len();
        rows.push(Row::new(
            ui.tr("Обновить настройки", "Refresh settings"),
            response["warning"].as_str().unwrap_or(""),
        ));
        let Some(index) = ui
            .choose(&module_name(ui.lang, module), &rows, &mut selected)
            .await?
        else {
            return Ok(());
        };
        let result = async {
            if index == 0 {
                return playlists(ui, module).await;
            }
            if index == refresh_index {
                response = ui.wait(module.config_json("settings", &[])).await?;
            } else if can_login && index == login_index {
                ui.suspend()?;
                let result = module.login().await;
                ui.resume()?;
                result?;
                response = ui.wait(module.config_json("settings", &[])).await?;
            } else {
                let key = &keys[index - 1];
                if module.manifest.id == "icecast" && key == "stations" {
                    stations(ui, module, &mut response).await?;
                } else if module.manifest.id == "icecast" && key == "station" {
                    let stations = settings["stations"]
                        .as_array()
                        .ok_or("Invalid station list")?;
                    let mut choice = stations
                        .iter()
                        .position(|s| s["id"] == settings["station"])
                        .unwrap_or(0);
                    let rows: Vec<_> = stations
                        .iter()
                        .map(|s| {
                            Row::new(
                                format!(
                                    "{} ({})",
                                    s["name"].as_str().unwrap_or(""),
                                    s["id"].as_str().unwrap_or("")
                                ),
                                "",
                            )
                        })
                        .collect();
                    if let Some(i) = ui
                        .choose(fields::label(ui.lang, key), &rows, &mut choice)
                        .await?
                    {
                        response = ui
                            .wait(set(
                                module,
                                key,
                                stations[i]["id"].as_str().ok_or("Invalid station ID")?,
                            ))
                            .await?;
                    }
                } else if let Some(value) =
                    fields::edit(ui, &module.manifest.id, key, &settings[key]).await?
                {
                    response = ui.wait(set(module, key, &value)).await?;
                }
            }
            Ok(())
        }
        .await;
        error(ui, result).await?;
    }
}
async fn playlists(ui: &mut Ui, module: &Module) -> Result<()> {
    let value = ui.wait(module.config_json("playlists", &[])).await?;
    let list = value["playlists"]
        .as_array()
        .ok_or("Invalid playlist response")?;
    let current = plugin::selected_playlist(module);
    let mut selected = list
        .iter()
        .position(|p| p["id"].as_str() == Some(&current))
        .unwrap_or(0);
    let mut rows: Vec<_> = list
        .iter()
        .map(|p| {
            Row::new(
                p["name"].as_str().unwrap_or("?"),
                p["id"].as_str().unwrap_or(""),
            )
        })
        .collect();
    if module.manifest.id == "icecast" {
        rows.insert(
            0,
            Row::new(
                ui.tr(
                    "По умолчанию (выбранная станция)",
                    "Default (selected station)",
                ),
                "default",
            ),
        );
        selected += usize::from(current != "default");
    }
    if let Some(index) = ui
        .choose(ui.tr("Плейлисты", "Playlists"), &rows, &mut selected)
        .await?
    {
        ui.wait(playlist(module, &rows[index].hint)).await?;
    }
    Ok(())
}
async fn stations(ui: &mut Ui, module: &Module, response: &mut Value) -> Result<()> {
    let mut selected = 0;
    loop {
        let stations = response["settings"]["stations"]
            .as_array()
            .ok_or("Invalid stations")?
            .clone();
        let mut rows: Vec<_> = stations
            .iter()
            .map(|s| {
                Row::new(
                    format!(
                        "{} ({})",
                        s["name"].as_str().unwrap_or(""),
                        s["id"].as_str().unwrap_or("")
                    ),
                    s["url"].as_str().unwrap_or(""),
                )
            })
            .collect();
        rows.push(Row::new(ui.tr("+ Добавить станцию", "+ Add station"), ""));
        let Some(index) = ui
            .choose(
                ui.tr("Радиостанции", "Radio stations"),
                &rows,
                &mut selected,
            )
            .await?
        else {
            return Ok(());
        };
        let result = async {
            if index == stations.len() {
                let mut station = json!({"id":"","name":"","url":"","username":"","password":""});
                for key in ["id", "name", "url"] {
                    let Some(value) = fields::edit(ui, "icecast", key, &station[key]).await? else {
                        return Ok(());
                    };
                    station[key] = json!(value);
                }
                *response = ui
                    .wait(set(module, "station_add", &station.to_string()))
                    .await?;
            } else {
                station_menu(
                    ui,
                    module,
                    response,
                    stations[index]["id"].as_str().ok_or("Invalid station ID")?,
                )
                .await?;
            }
            Ok(())
        }
        .await;
        error(ui, result).await?;
    }
}
async fn station_menu(ui: &mut Ui, module: &Module, response: &mut Value, id: &str) -> Result<()> {
    let mut id = id.to_string();
    let keys = [
        "id",
        "name",
        "url",
        "server",
        "mountpoint",
        "username",
        "password",
    ];
    let mut selected = 0;
    loop {
        let mut station = response["settings"]["stations"]
            .as_array()
            .and_then(|list| list.iter().find(|s| s["id"] == id))
            .ok_or("Station no longer exists")?
            .clone();
        if let Some(url) = station["url"]
            .as_str()
            .and_then(|value| url::Url::parse(value).ok())
        {
            station["mountpoint"] = json!(url.path());
            let mut server = url;
            server.set_path("");
            server.set_query(None);
            server.set_fragment(None);
            station["server"] = json!(server.as_str().trim_end_matches('/'));
        }
        let mut rows: Vec<_> = keys
            .iter()
            .map(|key| {
                Row::new(
                    format!(
                        "{}: {}",
                        fields::label(ui.lang, key),
                        fields::display(ui.lang, "icecast", key, &station[key])
                    ),
                    fields::hint(ui.lang, key),
                )
            })
            .collect();
        rows.push(Row::new(ui.tr("Удалить станцию", "Delete station"), ""));
        let Some(index) = ui
            .choose(
                &format!("{} / {id}", ui.tr("Станция", "Station")),
                &rows,
                &mut selected,
            )
            .await?
        else {
            return Ok(());
        };
        let result = async {
            if index == keys.len() {
                let rows = [
                    Row::new(ui.tr("Отмена", "Cancel"), ""),
                    Row::new(ui.tr("Удалить", "Delete"), ""),
                ];
                if ui
                    .choose(
                        ui.tr("Удалить эту станцию?", "Delete this station?"),
                        &rows,
                        &mut 0,
                    )
                    .await?
                    == Some(1)
                {
                    *response = ui.wait(set(module, "station_delete", &id)).await?;
                    if plugin::selected_playlist(module) == id {
                        ui.wait(playlist(module, "default")).await?;
                    }
                    return Ok(true);
                }
            } else {
                let key = keys[index];
                if let Some(value) = fields::edit(ui, "icecast", key, &station[key]).await? {
                    *response = ui
                        .wait(set(
                            module,
                            "station_update",
                            &json!({"id":id,"key":key,"value":value}).to_string(),
                        ))
                        .await?;
                    if key == "id" {
                        if id != "default" && plugin::selected_playlist(module) == id {
                            ui.wait(playlist(module, &value)).await?;
                        }
                        id = value;
                    }
                }
            }
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(err) => error(ui, Err(err)).await?,
        }
    }
}
