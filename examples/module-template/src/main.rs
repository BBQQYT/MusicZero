use std::io::{self, Write};

fn wav() -> io::Result<()> {
    const RATE: u32 = 48_000;
    const SAMPLES: u32 = RATE * 3;
    let bytes = SAMPLES * 2;
    let mut out = io::stdout().lock();
    out.write_all(b"RIFF")?;
    out.write_all(&(bytes + 36).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&RATE.to_le_bytes())?;
    out.write_all(&(RATE * 2).to_le_bytes())?;
    out.write_all(&2u16.to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&bytes.to_le_bytes())?;
    for n in 0..SAMPLES {
        let time = n as f32 / RATE as f32;
        let sample = (time * 440.0 * std::f32::consts::TAU).sin() * 0.15;
        out.write_all(&((sample * i16::MAX as f32) as i16).to_le_bytes())?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("info") => println!(r#"{{"protocol":1,"id":"demo","name":"Demo Tone","default_playlist":"demo"}}"#),
        Some("playlists") => println!(r#"{{"playlists":[{{"id":"demo","name":"Demo"}}]}}"#),
        Some("tracks") => println!(r#"{{"tracks":[{{"id":"tone","title":"440 Hz tone","artist":"Demo","duration_ms":3000}}]}}"#),
        Some("audio") if args.get(2).map(String::as_str) == Some("tone") => wav()?,
        Some("settings") => println!(r#"{{"settings":{{}}}}"#),
        Some("login") => {},
        _ => { eprintln!("Unknown command"); std::process::exit(1); }
    }
    Ok(())
}
