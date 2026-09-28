#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!(
            "ymz-tray v{} — иконка в системном трее для управления YMZ (Яндекс Музыка)",
            env!("CARGO_PKG_VERSION")
        );
        println!("Использование: ymz-tray");
        return Ok(());
    }
    if args
        .iter()
        .any(|a| a == "--version" || a == "-V" || a == "version")
    {
        println!("ymz-tray {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    env_logger::init();
    mcz::tray::run(mcz::tray::TrayConfig {
        id: "ymz",
        title: "YMZ — Яндекс Музыка",
        service: "org.mpris.MediaPlayer2.ymz",
        wave_settings: true,
    })
    .await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ymz-tray доступен только в Linux");
    std::process::exit(1);
}
