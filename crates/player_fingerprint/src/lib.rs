use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use player_error::{PlayerError, PlayerResult};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFingerprint {
    pub path: PathBuf,
    pub hash: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub frames: u64,
}

pub fn file_hash(path: impl AsRef<Path>) -> PlayerResult<String> {
    let path = path.as_ref();
    let mut file =
        File::open(path).map_err(|source| PlayerError::io(path.to_path_buf(), source))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 128 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| PlayerError::io(path.to_path_buf(), source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

pub fn audio_hash(path: impl AsRef<Path>) -> PlayerResult<AudioFingerprint> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| PlayerError::io(path.to_path_buf(), source))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| PlayerError::audio(format!("failed to probe audio: {error}")))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| PlayerError::audio("no default audio track found"))?;
    let track_id = track.id;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"normalplayer-audio-packet-hash-v2");

    let sample_rate_hz = track.codec_params.sample_rate.unwrap_or(0);
    let channels = track
        .codec_params
        .channels
        .map(|channels| channels.count())
        .unwrap_or(0);
    hasher.update(&sample_rate_hz.to_le_bytes());
    hasher.update(&(channels.min(u16::MAX as usize) as u16).to_le_bytes());

    let mut duration_units = 0_u64;
    let mut packet_count = 0_u64;
    let mut saw_audio_packet = false;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(PlayerError::audio(
                    "stream reset required while fingerprinting; dynamic tracks are not supported yet",
                ));
            }
            Err(error) => {
                return Err(PlayerError::audio(format!(
                    "failed to read audio packet: {error}"
                )));
            }
        };

        if packet.track_id() != track_id {
            continue;
        }
        if packet.data.is_empty() {
            continue;
        }
        saw_audio_packet = true;
        packet_count = packet_count.saturating_add(1);
        duration_units = duration_units.saturating_add(packet.dur);
        hasher.update(&packet.dur.to_le_bytes());
        hasher.update(&packet.trim_start.to_le_bytes());
        hasher.update(&packet.trim_end.to_le_bytes());
        hasher.update(&packet.data);
    }

    if !saw_audio_packet || packet_count == 0 {
        return Err(PlayerError::audio("no audio packets found"));
    }

    Ok(AudioFingerprint {
        path: path.to_path_buf(),
        hash: hasher.finalize().to_hex().to_string(),
        sample_rate_hz,
        channels,
        frames: duration_units.max(packet_count),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_hash_matches_identical_files() {
        let fixture = fixture("into_the_oceans_chorus.ogg");
        assert_eq!(file_hash(&fixture).unwrap(), file_hash(&fixture).unwrap());
    }

    #[test]
    fn audio_hash_matches_identical_audio_under_different_names() {
        let source = fixture("into_the_oceans_chorus.ogg");
        let dir = temp_dir("audio_hash_same");
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first title.ogg");
        let second = dir.join("second title.ogg");
        std::fs::copy(&source, &first).unwrap();
        std::fs::copy(&source, &second).unwrap();

        let first_hash = audio_hash(&first).unwrap();
        let second_hash = audio_hash(&second).unwrap();

        assert_eq!(first_hash.hash, second_hash.hash);
        assert!(first_hash.frames > 0);
        assert_eq!(first_hash.sample_rate_hz, second_hash.sample_rate_hz);
        assert_eq!(first_hash.channels, second_hash.channels);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn audio_hash_differs_for_different_recordings() {
        let first = audio_hash(fixture("into_the_oceans_chorus.ogg")).unwrap();
        let second = audio_hash(fixture("funk_room_reverb.ogg")).unwrap();
        assert_ne!(first.hash, second.hash);
    }

    #[test]
    fn audio_hash_ignores_wav_metadata_chunks() {
        let dir = temp_dir("wav_metadata");
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.wav");
        let second = dir.join("second.wav");
        write_test_wav(&first, b"first title").unwrap();
        write_test_wav(&second, b"second title").unwrap();

        assert_ne!(file_hash(&first).unwrap(), file_hash(&second).unwrap());
        assert_eq!(
            audio_hash(&first).unwrap().hash,
            audio_hash(&second).unwrap().hash
        );

        std::fs::remove_dir_all(dir).ok();
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("test-assets")
            .join("audio")
            .join(name)
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("player_fingerprint_{prefix}_{nonce}"))
    }

    fn write_test_wav(path: &Path, title: &[u8]) -> std::io::Result<()> {
        use std::io::Write;

        let sample_rate = 8_000_u32;
        let channels = 1_u16;
        let bits_per_sample = 16_u16;
        let sample_count = 800_u32;
        let block_align = channels * bits_per_sample / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = sample_count * u32::from(block_align);
        let title_padding = title.len() % 2;
        let list_payload_size = 4 + 8 + title.len() + title_padding;
        let list_padding = list_payload_size % 2;
        let list_size_with_padding = list_payload_size + list_padding;
        let riff_size = 4 + (8 + 16) + (8 + list_size_with_padding as u32) + (8 + data_size);

        let mut file = std::fs::File::create(path)?;
        file.write_all(b"RIFF")?;
        file.write_all(&riff_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;
        file.write_all(b"fmt ")?;
        file.write_all(&16_u32.to_le_bytes())?;
        file.write_all(&1_u16.to_le_bytes())?;
        file.write_all(&channels.to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        file.write_all(&byte_rate.to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&bits_per_sample.to_le_bytes())?;
        file.write_all(b"LIST")?;
        file.write_all(&(list_payload_size as u32).to_le_bytes())?;
        file.write_all(b"INFO")?;
        file.write_all(b"INAM")?;
        file.write_all(&(title.len() as u32).to_le_bytes())?;
        file.write_all(title)?;
        if title_padding == 1 {
            file.write_all(&[0])?;
        }
        if list_padding == 1 {
            file.write_all(&[0])?;
        }
        file.write_all(b"data")?;
        file.write_all(&data_size.to_le_bytes())?;
        for index in 0..sample_count {
            let sample = if index % 2 == 0 { 900_i16 } else { -900_i16 };
            file.write_all(&sample.to_le_bytes())?;
        }
        Ok(())
    }
}
