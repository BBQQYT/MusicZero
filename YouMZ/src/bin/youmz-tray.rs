#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!(
            "youmz-tray v{} — иконка в системном трее для управления YouMZ (YouTube Music)",
            env!("CARGO_PKG_VERSION")
        );
        println!("Использование: youmz-tray");
        return Ok(());
    }
    if args
        .iter()
        .any(|a| a == "--version" || a == "-V" || a == "version")
    {
        println!("youmz-tray {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    env_logger::init();
    mcz::tray::run(mcz::tray::TrayConfig {
        id: "youmz",
        title: "YouMZ — YouTube Music",
        service: "org.mpris.MediaPlayer2.youmz",
        wave_settings: false,
    })
    .await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("youmz-tray доступен только в Linux");
    std::process::exit(1);
}
