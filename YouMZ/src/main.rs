#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,youmz=info,mcz=info"),
    )
    .init();

    let args: Vec<String> = std::env::args().collect();
    let mut with_tray = true;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "login" | "--login" => return youmz::login().await,
            "help" | "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "version" | "--version" | "-V" => {
                println!("youmz {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--no-tray" => {
                with_tray = false;
            }
            unknown => {
                eprintln!("Неизвестная команда или флаг: {unknown}\n");
                print_help();
                std::process::exit(1);
            }
        }
    }

    youmz::run(with_tray, false).await
}

fn print_help() {
    println!(
        "YouMZ v{} — легковесный клиент YouTube Music\n\n\
Использование:\n  \
  youmz [КОМАНДА / ФЛАГИ]\n\n\
Команды и флаги:\n  \
  (без аргументов)    Запустить плеер (трей в Linux)\n  \
  login, --login      Войти в YouTube Music\n  \
  --no-tray           Запустить без трея в Linux\n  \
  help, --help, -h    Показать эту справку\n  \
  version, --version  Показать версию\n\n\
Управление воспроизведением:\n  \
  mz toggle / mz next / mz status\n  \
  или через трей в Linux",
        env!("CARGO_PKG_VERSION")
    );
}
