mod config;
mod feedback;
mod history;
mod ipc;
mod live;
mod notification;
mod output;
mod player;
mod plugin;
mod seek;
mod service;
#[cfg(all(feature = "tray", target_os = "linux"))]
mod tray;
mod tui;

use plugin::{discover, Module};
use serde_json::{json, Value};
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn module<'a>(modules: &'a [Module], id: &str) -> Result<&'a Module> {
    modules
        .iter()
        .find(|m| m.manifest.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("Модуль {id} не найден. Выполните `mz modules`.").into())
}

async fn command(action: &str, value: &str, key: &str) -> Result<Value> {
    let response = ipc::call(&json!({"action":action,"value":value,"key":key}))
        .await
        .map_err(|_| "MusicZero не запущен. Выполните `mz start <модуль>`.")?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(error.to_owned().into());
    }
    Ok(response)
}

fn show_status(value: &Value) {
    println!(
        "{} [{}] — {}",
        value["name"].as_str().unwrap_or("MusicZero"),
        value["module"].as_str().unwrap_or("?"),
        value["status"].as_str().unwrap_or("?")
    );
    let title = value["title"].as_str().unwrap_or("");
    if !title.is_empty() {
        println!("{} — {}", value["artist"].as_str().unwrap_or(""), title);
    }
    if value["can_seek"].as_bool().unwrap_or(false) {
        let position = value["position_ms"].as_u64().unwrap_or(0) / 1000;
        let duration = value["duration_ms"].as_u64().unwrap_or(0) / 1000;
        if duration > 0 {
            println!(
                "Позиция: {}:{:02} / {}:{:02}",
                position / 60,
                position % 60,
                duration / 60,
                duration % 60
            );
        } else {
            println!("Позиция: {}:{:02}", position / 60, position % 60);
        }
    }
    println!("Плейлист: {}", value["playlist"].as_str().unwrap_or("?"));
}

fn help() {
    println!(
        "MusicZero {} — один плеер, сменные модули\n\n\
  mz modules               Установленные модули\n\
  mz start <модуль>        Запустить плеер (также `mz <модуль>`)\n\
  mz switch <модуль>       Переключить источник без второго плеера\n\
  mz login <модуль>        Вход в сервис\n\
  mz status               Текущий трек\n\
  mz play|pause|toggle    Управление воспроизведением\n\
  mz seek <время>         Позиция: 90, 1:30; смещение: +15, -10\n\
  mz next|stop|quit       Следующий трек, остановка, выход\n\
  mz prev                 Предыдущий трек (до пяти назад)\n\
  mz history [модуль]     Последние пять треков, новые сверху\n\
  mz replay <номер>       Прослушать трек из истории\n\
  mz playlists            Список плейлистов\n\
  mz playlist <id|номер>  Выбрать плейлист\n\
  mz settings [модуль]    Настройки сервиса\n\
  mz config [модуль]      TUI настроек / Settings TUI (RU / EN)\n\
  mz tray [on|off]        Значок трея при следующем запуске (Linux)\n\
  mz service install <модуль> Установить сервис с автозапуском (Linux)\n\
  mz service start|stop|restart|status|remove Управление сервисом\n\
  mz set [модуль] <ключ> <значение> Изменить настройку\n\n\
Папки модулей лежат в `modules` рядом с mz.",
        env!("CARGO_PKG_VERSION")
    );
}

fn validate_args(args: &[String]) -> Result<()> {
    let Some(action) = args.first().map(String::as_str) else {
        return Ok(());
    };
    let valid = match action {
        "set" => matches!(args.len(), 3 | 4),
        "settings" | "playlist" | "config" | "tui" | "history" => matches!(args.len(), 1 | 2),
        "wave" => matches!(args.len(), 1 | 3),
        "tray" => matches!(args.len(), 1 | 2),
        "service" => match args.get(1).map(String::as_str) {
            Some("install") => args.len() == 3,
            Some("start" | "stop" | "restart" | "status" | "remove") => args.len() == 2,
            _ => false,
        },
        "start" | "serve" | "switch" | "login" | "seek" | "replay" => args.len() == 2,
        "modules" | "status" | "play" | "resume" | "pause" | "toggle" | "pp" | "next" | "skip"
        | "prev" | "previous" | "stop" | "quit" | "playlists" | "help" | "--help" | "-h"
        | "version" | "--version" | "-V" => args.len() == 1,
        _ => args.len() == 1,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Неверные аргументы команды {action}. Выполните `mz help`.").into())
    }
}

async fn start(modules: Vec<Module>, id: &str, managed: bool) -> Result<()> {
    let index = modules
        .iter()
        .position(|m| m.manifest.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("Модуль {id} не найден"))?;
    modules[index].validate().await?;
    if ipc::call(&json!({"action":"ping"})).await.is_ok() {
        if managed {
            return Err(
                "Плеер уже запущен вручную. Выполните `mz quit`, затем `mz service start`.".into(),
            );
        }
        command("switch", &modules[index].manifest.id, "").await?;
        println!("Источник: {}", modules[index].manifest.name);
        return Ok(());
    }
    println!("▶ {}. Ctrl+C для выхода.", modules[index].manifest.name);
    #[cfg(all(feature = "tray", target_os = "linux"))]
    if config::Settings::load()?.tray_enabled {
        tray::spawn(modules.clone());
    }
    player::run(modules, index).await
}

#[tokio::main]
async fn main() -> Result<()> {
    let preferences = config::Settings::load()?;
    let log_filter = if preferences.log_filter.is_empty() {
        "warn,mz=info"
    } else {
        &preferences.log_filter
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_filter)).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    validate_args(&args)?;
    // Help/version must work even when module discovery is unavailable.
    match args.first().map(String::as_str) {
        Some("help" | "--help" | "-h") => {
            help();
            return Ok(());
        }
        Some("version" | "--version" | "-V") => {
            println!("mz {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("config" | "tui") => return tui::run(args.get(1).map(String::as_str)).await,
        Some("tray") => {
            if let Some(value) = args.get(1) {
                if !cfg!(all(feature = "tray", target_os = "linux")) {
                    return Err("Этот файл mz собран без поддержки трея Linux".into());
                }
                let mut settings = preferences.clone();
                settings.set(
                    "tray_enabled",
                    match value.as_str() {
                        "on" => "true",
                        "off" => "false",
                        _ => return Err("Используйте `mz tray on` или `mz tray off`".into()),
                    },
                )?;
                settings.save()?;
                println!(
                    "{}",
                    settings.language.text(
                        "Настройка трея сохранена; перезапустите плеер.",
                        "Tray preference saved; restart the player."
                    )
                );
            } else {
                println!(
                    "{}",
                    if preferences.tray_enabled && cfg!(all(feature = "tray", target_os = "linux"))
                    {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            return Ok(());
        }
        Some("service") => return service::cli(&args[1..]).await,
        _ => {}
    }
    let modules = discover()?;
    match args.first().map(String::as_str) {
        None => {
            if let Ok(status) = command("status", "", "").await {
                show_status(&status);
            } else {
                help();
            }
        }
        Some("modules") => {
            if modules.is_empty() {
                println!("Модулей нет. Создайте папку modules рядом с mz.");
            }
            for item in &modules {
                println!("{} — {}", item.manifest.id, item.manifest.name);
            }
        }
        Some("start" | "serve") => {
            start(
                modules,
                args.get(1).map(String::as_str).ok_or("Укажите модуль")?,
                args[0] == "serve",
            )
            .await?
        }
        Some("switch") => {
            let id = args.get(1).ok_or("Укажите модуль")?;
            let selected = module(&modules, id)?;
            command("switch", &selected.manifest.id, "").await?;
            println!("Источник: {}", selected.manifest.name);
        }
        Some("login") => {
            module(
                &modules,
                args.get(1).map(String::as_str).ok_or("Укажите модуль")?,
            )?
            .login()
            .await?;
        }
        Some("seek") => {
            let (value, mode) = seek::SeekRequest::parse(&args[1])?.wire();
            let response = command("seek", &value, mode).await?;
            let seconds = response["position_ms"].as_u64().unwrap_or(0) / 1000;
            println!("Позиция: {}:{:02}", seconds / 60, seconds % 60);
        }
        Some("status") => show_status(&command("status", "", "").await?),
        Some("play" | "resume") => {
            command("play", "", "").await?;
            println!("▶ Воспроизведение");
        }
        Some("pause") => {
            command("pause", "", "").await?;
            println!("⏸ Пауза");
        }
        Some("toggle" | "pp") => show_status(&command("toggle", "", "").await?),
        Some("next" | "skip") => {
            command("next", "", "").await?;
            println!("⏭ Следующий трек");
        }
        Some("prev" | "previous") => {
            command("previous", "", "").await?;
            println!("⏮ Предыдущий трек");
        }
        Some("history") => {
            let tracks: Vec<plugin::Track> = if let Some(id) = args.get(1) {
                let selected = module(&modules, id)?;
                match command("history", "", "").await {
                    Ok(response) if response["module"] == selected.manifest.id => {
                        serde_json::from_value(response["tracks"].clone())?
                    }
                    _ => history::History::load(&selected.manifest.id).entries(None),
                }
            } else {
                serde_json::from_value(command("history", "", "").await?["tracks"].clone())?
            };
            if tracks.is_empty() {
                println!("История треков пока пуста.");
            }
            for (index, track) in tracks.iter().enumerate() {
                println!("{}. {} — {}", index + 1, track.artist, track.title);
            }
        }
        Some("replay") => {
            command("replay", &args[1], "").await?;
            println!("▶ Трек из истории");
        }
        Some("stop") => {
            command("stop", "", "").await?;
            println!("⏹ Остановлено");
        }
        Some("quit") => {
            command("quit", "", "").await?;
            println!("MusicZero завершён");
        }
        Some("playlists") => {
            let response = command("playlists", "", "").await?;
            let current = response["playlist"].as_str().unwrap_or("");
            if let Some(list) = response["playlists"].as_array() {
                for (index, item) in list.iter().enumerate() {
                    let id = item["id"].as_str().unwrap_or("");
                    let mark = if id == current { "*" } else { " " };
                    println!(
                        "{mark} {:>2}. {} ({id})",
                        index + 1,
                        item["name"].as_str().unwrap_or("")
                    );
                }
            }
        }
        Some("playlist") => {
            if let Some(selector) = args.get(1) {
                let mut id = selector.clone();
                if let Ok(number) = selector.parse::<usize>() {
                    let response = command("playlists", "", "").await?;
                    id = response["playlists"]
                        .as_array()
                        .and_then(|list| number.checked_sub(1).and_then(|index| list.get(index)))
                        .and_then(|item| item["id"].as_str())
                        .ok_or("Номер плейлиста вне диапазона")?
                        .into();
                }
                command("playlist", &id, "").await?;
                println!("Плейлист: {id}");
            } else {
                let response = command("playlists", "", "").await?;
                println!(
                    "Текущий плейлист: {}",
                    response["playlist"].as_str().unwrap_or("?")
                );
            }
        }
        Some("settings") if args.len() == 2 => {
            let selected = module(&modules, &args[1])?;
            let response = selected.json("settings", &[]).await?;
            println!("{}", serde_json::to_string_pretty(&response["settings"])?);
        }
        Some("set") if args.len() == 4 => {
            let selected = module(&modules, &args[1])?;
            let active = command("status", "", "")
                .await
                .ok()
                .is_some_and(|status| status["module"] == selected.manifest.id);
            let response = if active {
                command("set-setting", &args[3], &args[2]).await?
            } else {
                selected.json("set-setting", &[&args[2], &args[3]]).await?
            };
            println!("{}", serde_json::to_string_pretty(&response["settings"])?);
        }
        Some("settings" | "wave") if args.len() < 3 => {
            let response = command("settings", "", "").await?;
            println!("{}", serde_json::to_string_pretty(&response["settings"])?);
        }
        Some("set" | "wave") => {
            let key = args.get(1).ok_or("Укажите ключ")?;
            let value = args.get(2).ok_or("Укажите значение")?;
            let response = command("set-setting", value, key).await?;
            println!("{}", serde_json::to_string_pretty(&response["settings"])?);
        }
        Some("all") => {
            return Err(
                "Теперь одновременно работает один источник. Используйте `mz switch <модуль>`."
                    .into(),
            )
        }
        Some(id)
            if modules
                .iter()
                .any(|item| item.manifest.id.eq_ignore_ascii_case(id)) =>
        {
            start(modules, id, false).await?
        }
        Some(other) => return Err(format!("Неизвестная команда или модуль: {other}").into()),
    }
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn setting_arguments_cannot_fall_through_to_another_command_form() {
        let args = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for values in [
            vec!["set"],
            vec!["set", "path"],
            vec!["set", "local", "path", "/music", "extra"],
            vec!["settings", "local", "extra"],
            vec!["next", "extra"],
        ] {
            assert!(validate_args(&args(&values)).is_err());
        }
        for values in [
            vec!["set", "local", "path", "/music"],
            vec!["set", "path", "/music"],
            vec!["settings", "local"],
            vec!["settings"],
            vec!["local"],
        ] {
            assert!(validate_args(&args(&values)).is_ok());
        }
    }
}
