mod ipc;
mod player;
mod plugin;
#[cfg(all(feature = "tray", target_os = "linux"))]
mod tray;

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
  mz next|stop|quit       Следующий трек, остановка, выход\n\
  mz playlists            Список плейлистов\n\
  mz playlist <id|номер>  Выбрать плейлист\n\
  mz settings             Настройки активного сервиса\n\
  mz set <ключ> <значение> Изменить настройку\n\n\
Папки модулей лежат в `modules` рядом с mz.",
        env!("CARGO_PKG_VERSION")
    );
}

async fn start(modules: Vec<Module>, id: &str) -> Result<()> {
    let index = modules
        .iter()
        .position(|m| m.manifest.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("Модуль {id} не найден"))?;
    modules[index].validate().await?;
    if ipc::call(&json!({"action":"ping"})).await.is_ok() {
        command("switch", &modules[index].manifest.id, "").await?;
        println!("Источник: {}", modules[index].manifest.name);
        return Ok(());
    }
    println!("▶ {}. Ctrl+C для выхода.", modules[index].manifest.name);
    #[cfg(all(feature = "tray", target_os = "linux"))]
    tray::spawn(modules.clone());
    player::run(modules, index).await
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,mz=info"))
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let modules = discover()?;
    match args.first().map(String::as_str) {
        None => {
            if let Ok(status) = command("status", "", "").await {
                show_status(&status);
            } else {
                help();
            }
        }
        Some("help" | "--help" | "-h") => help(),
        Some("version" | "--version" | "-V") => println!("mz {}", env!("CARGO_PKG_VERSION")),
        Some("modules") => {
            if modules.is_empty() {
                println!("Модулей нет. Создайте папку modules рядом с mz.");
            }
            for item in &modules {
                println!("{} — {}", item.manifest.id, item.manifest.name);
            }
        }
        Some("start") => {
            start(
                modules,
                args.get(1).map(String::as_str).ok_or("Укажите модуль")?,
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
            start(modules, id).await?
        }
        Some(other) => return Err(format!("Неизвестная команда или модуль: {other}").into()),
    }
    Ok(())
}
