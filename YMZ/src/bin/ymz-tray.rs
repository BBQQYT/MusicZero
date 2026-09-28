#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    mcz::tray::run(mcz::tray::TrayConfig {
        id: "ymz",
        title: "YMZ — Яндекс Музыка",
        service: "org.mpris.MediaPlayer2.ymz",
        wave_settings: true,
    })
    .await
}
