mod client;

use client::{DbusClient, StatusInfo, YMZ_SERVICE};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,mz=info,ymz=info,youmz=info,mcz=info"),
    )
    .init();

    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        return handle_default().await;
    }

    let cmd = args[1].as_str();
    let rest: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match cmd {
        // Запуск сервисов
        "ymz" | "yandex" => run_ymz(&rest).await,
        "youmz" | "youtube" => run_youmz(&rest).await,
        "all" | "start-all" => run_all(&rest).await,
        "start" => {
            let target = rest.first().copied().unwrap_or("ymz");
            if target == "youmz" || target == "youtube" {
                run_youmz(&rest[1..]).await
            } else if target == "all" {
                run_all(&rest[1..]).await
            } else {
                run_ymz(&rest[1..]).await
            }
        }

        // Авторизация
        "login" => youmz::login().await,

        // Управление воспроизведением
        "play" | "resume" => handle_playback("play", &rest).await,
        "pause" => handle_playback("pause", &rest).await,
        "toggle" | "play-pause" | "pp" => handle_playback("toggle", &rest).await,
        "next" | "skip" => handle_playback("next", &rest).await,
        "prev" | "previous" => handle_playback("prev", &rest).await,
        "stop" => handle_playback("stop", &rest).await,

        // Статус
        "status" | "current" | "now" => handle_status(&rest).await,

        // Плейлисты
        "playlists" => handle_playlists(&rest).await,
        "playlist" => {
            if rest.is_empty() || rest[0] == "list" {
                handle_playlists(&rest).await
            } else {
                handle_set_playlist(&rest).await
            }
        }

        // Настройки волны (YMZ)
        "wave" => handle_wave(&rest).await,

        // Отдельный трей
        "tray" => handle_tray(&rest).await,

        // Справка и версия
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("mz {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }

        unknown => {
            eprintln!("Неизвестная команда или флаг: {unknown}\n");
            print_help();
            std::process::exit(1);
        }
    }
}

fn with_tray_flag(args: &[&str]) -> bool {
    !args.contains(&"--no-tray")
}

async fn run_ymz(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let with_tray = with_tray_flag(args);
    println!("▶ Запуск Яндекс Музыки (YMZ)...{}", if with_tray { " (с треем)" } else { "" });
    ymz::run(with_tray).await
}

async fn run_youmz(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let with_tray = with_tray_flag(args);
    println!("▶ Запуск YouTube Music (YouMZ)...{}", if with_tray { " (с треем)" } else { "" });
    youmz::run(with_tray).await
}

async fn run_all(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let with_tray = with_tray_flag(args);
    println!("▶ Запуск YMZ и YouMZ одновременно...{}", if with_tray { " (с треями)" } else { "" });
    tokio::select! {
        res1 = ymz::run(with_tray) => res1,
        res2 = youmz::run(with_tray) => res2,
    }
}

async fn handle_playback(action: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    let preferred = args.first().copied();

    let Some((service, label)) = client.resolve_service(preferred).await else {
        eprintln!("Ни один плеер MusicZero не запущен.");
        eprintln!("Запустите плеер: `mz ymz` или `mz youmz`");
        return Ok(());
    };

    match action {
        "play" => {
            client.play(service).await?;
            println!("▶ Воспроизведение [{label}]");
        }
        "pause" => {
            client.pause(service).await?;
            println!("⏸ Пауза [{label}]");
        }
        "toggle" => {
            client.play_pause(service).await?;
            let status = client.get_playback_status(service).await.unwrap_or_default();
            println!("⏯ Play/Pause: {status} [{label}]");
        }
        "next" => {
            client.next(service).await?;
            println!("⏭ Следующий трек [{label}]");
        }
        "prev" => {
            client.previous(service).await?;
            println!("⏮ Предыдущий трек [{label}]");
        }
        "stop" => {
            client.stop(service).await?;
            println!("⏹ Остановлено [{label}]");
        }
        _ => {}
    }

    Ok(())
}

async fn handle_status(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    let preferred = args.first().copied();

    let Some((service, label)) = client.resolve_service(preferred).await else {
        println!("Ни один плеер MusicZero сейчас не запущен.");
        println!("Запустите: `mz ymz` (Яндекс) или `mz youmz` (YouTube)");
        return Ok(());
    };

    let info = client.get_status(service, label).await?;
    print_status_info(&info);
    Ok(())
}

fn print_status_info(info: &StatusInfo) {
    let icon = match info.status.as_str() {
        "Playing" => "▶ Воспроизведение",
        "Paused" => "⏸ На паузе",
        _ => "⏹ Остановлен",
    };

    println!("═══════════════════════════════════════════");
    println!("  Сервис:    {}", info.service_label);
    println!("  Статус:    {}", icon);
    if !info.title.is_empty() {
        println!("  Трек:      {} — {}", info.artist, info.title);
    }
    println!("  Плейлист:  {} ({})", info.playlist_name, info.playlist_id);

    if let Some((mood, div, lang)) = &info.wave_settings {
        println!("  Волна:     настроение: {mood} | разнообразие: {div} | язык: {lang}");
    }
    println!("═══════════════════════════════════════════");
}

async fn handle_playlists(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    let preferred = args.first().copied();

    let Some((service, label)) = client.resolve_service(preferred).await else {
        eprintln!("Плеер не запущен. Запустите: `mz ymz` или `mz youmz`");
        return Ok(());
    };

    let (current, list) = client.list_playlists(service).await?;
    println!("Плейлисты для [{}]:", label);
    for (i, (id, name)) in list.iter().enumerate() {
        let active = if id == &current { " *" } else { "  " };
        println!("{active} {:>2}. {:<20} (ID: {})", i + 1, name, id);
    }
    println!("\nДля переключения: `mz playlist <номер или ID>`");
    Ok(())
}

async fn handle_set_playlist(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    let selector = args[0];
    let preferred = args.get(1).copied();

    let Some((service, label)) = client.resolve_service(preferred).await else {
        eprintln!("Плеер не запущен. Запустите: `mz ymz` или `mz youmz`");
        return Ok(());
    };

    let (_, list) = client.list_playlists(service).await?;
    let target_id = if let Ok(num) = selector.parse::<usize>() {
        if num >= 1 && num <= list.len() {
            list[num - 1].0.clone()
        } else {
            eprintln!("Номер плейлиста вне диапазона: 1..{}", list.len());
            return Ok(());
        }
    } else {
        selector.to_string()
    };

    let name = list.iter().find(|(id, _)| id == &target_id).map(|(_, n)| n.as_str()).unwrap_or(&target_id);
    client.set_playlist(service, &target_id).await?;
    println!("🔀 Плейлист переключён на: «{}» (ID: {}) [{label}]", name, target_id);
    Ok(())
}

async fn handle_wave(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    if !client.is_service_running(YMZ_SERVICE).await {
        eprintln!("Яндекс Музыка (YMZ) не запущена.");
        return Ok(());
    }

    if args.is_empty() {
        let (mood, div, lang) = client.get_wave_settings().await?;
        println!("Текущие настройки «Моей волны» (YMZ):");
        println!("  Настроение (mood):        {mood}");
        println!("  Разнообразие (diversity): {div}");
        println!("  Язык (language):          {lang}");
        println!("\nИзменение настройки: `mz wave <mood|diversity|language> <значение>`");
        println!("Доступные значения:");
        println!("  mood:      all, energetic, calm, sad, joyful");
        println!("  diversity: default, favorite, discover, popular");
        println!("  language:  any, russian, not-russian");
        return Ok(());
    }

    if args.len() >= 2 {
        let key = args[0];
        let val = args[1];
        client.set_wave_setting(key, val).await?;
        println!("✔ Настройка волны обновлена: {key} = {val}");
    } else {
        eprintln!("Использование: `mz wave <ключ> <значение>`");
    }

    Ok(())
}

async fn handle_tray(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let target = args.first().copied().unwrap_or("ymz");
    if target == "youmz" || target == "youtube" {
        println!("Запуск трея YouMZ...");
        mcz::tray::run(mcz::tray::TrayConfig {
            id: "youmz",
            title: "YouMZ — YouTube Music",
            service: "org.mpris.MediaPlayer2.youmz",
            wave_settings: false,
        })
        .await
    } else {
        println!("Запуск трея YMZ...");
        mcz::tray::run(mcz::tray::TrayConfig {
            id: "ymz",
            title: "YMZ — Яндекс Музыка",
            service: "org.mpris.MediaPlayer2.ymz",
            wave_settings: true,
        })
        .await
    }
}

async fn handle_default() -> Result<(), Box<dyn std::error::Error>> {
    let client = DbusClient::new().await?;
    if let Some((service, label)) = client.resolve_service(None).await {
        let info = client.get_status(service, label).await?;
        print_status_info(&info);
        println!("\nБыстрые команды:");
        println!("  mz toggle         Play / Pause");
        println!("  mz next / prev    Следующий / предыдущий трек");
        println!("  mz playlists      Список плейлистов");
        println!("  mz help           Все команды");
    } else {
        print_help();
    }
    Ok(())
}

fn print_help() {
    println!(
        "MusicZero (mz) v{} — единая модульная система управления и запуска музыки\n\n\
Использование:\n  \
  mz <КОМАНДА> [ПАРАМЕТРЫ]\n\n\
Запуск плееров (сразу с треем):\n  \
  mz ymz              Запустить Яндекс Музыку (+ трей)\n  \
  mz youmz            Запустить YouTube Music (+ трей)\n  \
  mz all              Запустить оба сервиса одновременно\n  \
  mz login            Войти в аккаунт YouTube Music\n  \
  --no-tray           (флаг к запуску) запустить без трея\n\n\
Управление воспроизведением:\n  \
  mz play             Возобновить воспроизведение\n  \
  mz pause            Поставить на паузу\n  \
  mz toggle (или pp)  Переключить Play/Pause\n  \
  mz next (или skip)  Следующий трек\n  \
  mz prev             Предыдущий трек\n  \
  mz stop             Остановить воспроизведение\n  \
  mz status           Показать текущий трек и статус\n\n\
Плейлисты и настройки:\n  \
  mz playlists        Показать список доступных плейлистов\n  \
  mz playlist <N|id>  Переключить плейлист по номеру или ID\n  \
  mz wave [опции]     Настройки «Моей волны» Яндекс Музыки\n\n\
Совет: команды управления автоматически выбирают активный плеер.\n\
Вы можете явно указать сервис: например `mz next ymz` или `mz pause youmz`.",
        env!("CARGO_PKG_VERSION")
    );
}
