use crate::{auth, config::Settings, Result};
use librespot_audio::{AudioDecrypt, AudioFile};
use librespot_core::{
    authentication::Credentials, config::SessionConfig, spotify_id::SpotifyId,
    spotify_uri::SpotifyUri, FileId, Session,
};
use librespot_metadata::audio::{AudioFileFormat, AudioItem};
use std::io::{Read, Seek as _};
use std::time::Duration;

// Spotify prefixes each decrypted Ogg Vorbis stream with a 167-byte metadata
// packet; librespot's player skips it the same way after parsing it.
const OGG_HEADER_END: u64 = 0xa7;

pub fn stream_uri(id: &str) -> Result<SpotifyUri> {
    // The host passes back the `track:<id>` or `episode:<id>` from `tracks`.
    if !id.starts_with("track:") && !id.starts_with("episode:") {
        return Err("Spotify audio expects a track:<id> or episode:<id>".into());
    }
    SpotifyUri::from_uri(crate::api::spotify_uri(&format!("spotify:{id}"))?)
        .map_err(|_| "Invalid Spotify audio id".into())
}

fn preferred_formats(bitrate: u32) -> [AudioFileFormat; 13] {
    // The requested quality first, then a reliable fallback chain; podcasts ship
    // as MP4/AAC so Vorbis and episode encodings are all candidates.
    match bitrate {
        96 => [
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::MP4_128,
            AudioFileFormat::AAC_48,
            AudioFileFormat::AAC_320,
            AudioFileFormat::AAC_160,
            AudioFileFormat::XHE_AAC_24,
            AudioFileFormat::XHE_AAC_16,
            AudioFileFormat::OTHER5,
            AudioFileFormat::MP3_320,
            AudioFileFormat::MP3_256,
            AudioFileFormat::MP3_160,
        ],
        320 => [
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::MP4_128,
            AudioFileFormat::AAC_48,
            AudioFileFormat::AAC_320,
            AudioFileFormat::AAC_160,
            AudioFileFormat::XHE_AAC_24,
            AudioFileFormat::XHE_AAC_16,
            AudioFileFormat::OTHER5,
            AudioFileFormat::MP3_320,
            AudioFileFormat::MP3_256,
            AudioFileFormat::MP3_160,
        ],
        _ => [
            AudioFileFormat::OGG_VORBIS_160,
            AudioFileFormat::OGG_VORBIS_320,
            AudioFileFormat::OGG_VORBIS_96,
            AudioFileFormat::MP4_128,
            AudioFileFormat::AAC_48,
            AudioFileFormat::AAC_320,
            AudioFileFormat::AAC_160,
            AudioFileFormat::XHE_AAC_24,
            AudioFileFormat::XHE_AAC_16,
            AudioFileFormat::OTHER5,
            AudioFileFormat::MP3_320,
            AudioFileFormat::MP3_256,
            AudioFileFormat::MP3_160,
        ],
    }
}

fn bytes_per_second(format: AudioFileFormat) -> usize {
    let kbps = match format {
        AudioFileFormat::OGG_VORBIS_96 | AudioFileFormat::MP3_96 => 12.0,
        AudioFileFormat::OGG_VORBIS_160
        | AudioFileFormat::MP3_160
        | AudioFileFormat::MP3_160_ENC
        | AudioFileFormat::AAC_160 => 20.0,
        AudioFileFormat::OGG_VORBIS_320
        | AudioFileFormat::MP3_256
        | AudioFileFormat::MP3_320
        | AudioFileFormat::AAC_320
        | AudioFileFormat::OTHER5 => 40.0,
        AudioFileFormat::AAC_24 => 3.0,
        AudioFileFormat::AAC_48 => 6.0,
        AudioFileFormat::MP4_128 => 16.0,
        AudioFileFormat::FLAC_FLAC => 112.0,
        AudioFileFormat::XHE_AAC_12 => 1.5,
        AudioFileFormat::XHE_AAC_16 => 2.0,
        AudioFileFormat::XHE_AAC_24 => 3.0,
        AudioFileFormat::FLAC_FLAC_24BIT => 3.0,
    };
    (kbps * 1024.0_f64).ceil() as usize
}

fn choose_format(item: &AudioItem, bitrate: u32) -> Option<(AudioFileFormat, FileId)> {
    preferred_formats(bitrate)
        .into_iter()
        .find_map(|format| item.files.get(&format).map(|file| (format, *file)))
}

async fn with_alternatives(
    session: &Session,
    uri: SpotifyUri,
    bitrate: u32,
) -> Result<(AudioFileFormat, FileId)> {
    // Following librespot's player: when the original recording is lacking but
    // the track has alternatives, try up to eight of them before giving up.
    let mut first_error = None;
    let mut queue = vec![uri];
    for _ in 0..8 {
        let uri = queue.remove(0);
        let item = match AudioItem::get_file(session, uri.clone()).await {
            Ok(item) => item,
            Err(error) => {
                first_error = first_error.or_else(|| Some(String::new() + &error.to_string()));
                continue;
            }
        };
        if let Err(reason) = item.availability {
            first_error = first_error.or_else(|| Some(format!("{reason}")));
        } else if let Some((format, file)) = choose_format(&item, bitrate) {
            return Ok((format, file));
        }
        if let Some(alternatives) = item.alternatives {
            queue.extend(alternatives.0);
        }
    }
    Err(first_error
        .map(|reason| format!("Spotify track is unavailable: {reason}"))
        .unwrap_or_else(|| "Spotify track is unavailable in any supported format".to_string())
        .into())
}

pub struct TrackStream {
    source: AudioDecrypt<AudioFile>,
    length: u64,
    position: u64,
}

impl Read for TrackStream {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.length - self.position;
        let window = output.len().min(remaining.min(usize::MAX as u64) as usize);
        let count = self.source.read(&mut output[..window])?;
        self.position += count as u64;
        Ok(count)
    }
}

pub async fn audio_stream(id: &str, settings: &Settings) -> Result<TrackStream> {
    settings.validate()?;
    if settings.client_id.is_empty() {
        return Err("Set your Spotify app client_id first: mz set spotify client_id <id>".into());
    }
    let uri = stream_uri(id)?;
    let mut tokens = auth::Tokens::load().map_err(|_| "Spotify: run `mz login spotify` first")?;
    if tokens.client_id != settings.client_id {
        return Err("Spotify client_id changed; sign in again with `mz login spotify`".into());
    }
    if !tokens.scope.is_empty() && !tokens.scope.split(' ').any(|s| s == "streaming") {
        return Err("Spotify session lacks the streaming scope; sign in again".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let token = tokens.fresh(&client, settings).await?;
    let track_id = SpotifyId::try_from(&uri)?;

    let config = SessionConfig {
        tmp_dir: std::env::temp_dir(),
        ..Default::default()
    };
    let session = Session::new(config, None);
    let connect = tokio::time::timeout(
        Duration::from_secs(90),
        session.connect(Credentials::with_access_token(token), false),
    )
    .await;
    match connect {
        Err(_) => return Err("Spotify session did not connect within 90 seconds".into()),
        Ok(result) => result.map_err(|e| format!("Spotify session: {e}"))?,
    }

    let (format, file_id) = with_alternatives(&session, uri, settings.bitrate).await?;
    let key = match tokio::time::timeout(
        Duration::from_secs(30),
        session.audio_key().request(track_id, file_id),
    )
    .await
    {
        Err(_) => return Err("Spotify audio key did not arrive within 30 seconds".into()),
        Ok(result) => result.map_err(|e| format!("Spotify audio key: {e}"))?,
    };

    let file = AudioFile::open(&session, file_id, bytes_per_second(format))
        .await
        .map_err(|e| format!("Spotify audio download failed: {e}"))?;
    let controller = file.get_stream_loader_controller()?;
    let length = controller.len() as u64;

    let offset = if format == AudioFileFormat::OGG_VORBIS_320
        || format == AudioFileFormat::OGG_VORBIS_160
        || format == AudioFileFormat::OGG_VORBIS_96
    {
        OGG_HEADER_END
    } else {
        0
    };
    let mut source = AudioDecrypt::new(Some(key), file);
    source
        .seek(std::io::SeekFrom::Start(offset))
        .map_err(|e| format!("Spotify audio header skip failed: {e}"))?;
    controller.set_stream_mode();
    Ok(TrackStream {
        source,
        length: length - offset,
        position: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_id_rejects_non_audio_kinds() {
        assert!(stream_uri("track:0123456789012345678901").is_ok());
        assert!(stream_uri("episode:0123456789012345678901").is_ok());
        for id in [
            "album:0123456789012345678901",
            "playlist:0123456789012345678901",
            "0123456789012345678901",
            "track:-invalid",
            "track:012345678901234567890!",
        ] {
            assert!(stream_uri(id).is_err(), "{id}");
        }
    }

    #[test]
    fn bitrate_prefers_the_requested_vorbis_quality() {
        for (bitrate, expected) in [
            (320, AudioFileFormat::OGG_VORBIS_320),
            (160, AudioFileFormat::OGG_VORBIS_160),
            (96, AudioFileFormat::OGG_VORBIS_96),
        ] {
            assert_eq!(preferred_formats(bitrate)[0], expected, "{bitrate}");
        }
        assert_eq!(preferred_formats(160).len(), 13);
        assert!(preferred_formats(160).contains(&AudioFileFormat::MP4_128));
    }

    #[test]
    fn vorbis_rates_match_librespot_stream_estimates() {
        assert_eq!(bytes_per_second(AudioFileFormat::OGG_VORBIS_96), 12288);
        assert_eq!(bytes_per_second(AudioFileFormat::OGG_VORBIS_160), 20480);
        assert_eq!(bytes_per_second(AudioFileFormat::OGG_VORBIS_320), 40960);
        assert_eq!(bytes_per_second(AudioFileFormat::MP4_128), 16384);
    }
}
