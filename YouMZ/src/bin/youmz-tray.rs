#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    mcz::tray::run(mcz::tray::TrayConfig {
        id: "youmz",
        title: "YouMZ — YouTube Music",
        service: "org.mpris.MediaPlayer2.youmz",
        wave_settings: false,
    })
    .await
}
