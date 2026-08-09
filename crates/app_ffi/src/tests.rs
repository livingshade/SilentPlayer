use super::*;
use crate::dto::{track_dtos, track_to_dto, LibraryPackageManifest, LibraryPackageTrack};
use crate::ffi::*;
use crate::file_support::{read_artwork_image, sqlite_database_files};
use crate::playback_helpers::library_playback_plan;
use crate::support::path_to_string_lossy;
use domain::{
    ArtworkImage, NormalizationSettings, RepeatMode, Track, TrackId, TrackViewId, TrackViewKind,
};
use engine::{AudioBackend, AudioRenderSettings, PlaybackEvent, PlayerEngine};
use errors::{PlayerError, PlayerResult};
use fingerprint::{audio_hash, file_hash};
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::{c_char, CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};
use store_sqlite::LibraryStore;

struct UnloadedBackend;
struct LoadedBackend;

impl AudioBackend for UnloadedBackend {
    fn load(&mut self, _track: &Track, _settings: AudioRenderSettings) -> PlayerResult<()> {
        Ok(())
    }

    fn play(&mut self) -> PlayerResult<()> {
        Ok(())
    }

    fn pause(&mut self) -> PlayerResult<()> {
        Err(PlayerError::audio("no track loaded"))
    }

    fn seek_to(&mut self, _position_ms: u64) -> PlayerResult<()> {
        Ok(())
    }

    fn set_gain(&mut self, _gain: domain::GainDecision) -> PlayerResult<()> {
        Ok(())
    }

    fn position_ms(&self) -> PlayerResult<u64> {
        Ok(0)
    }
}

impl AudioBackend for LoadedBackend {
    fn load(&mut self, _track: &Track, _settings: AudioRenderSettings) -> PlayerResult<()> {
        Ok(())
    }

    fn play(&mut self) -> PlayerResult<()> {
        Ok(())
    }

    fn pause(&mut self) -> PlayerResult<()> {
        Ok(())
    }

    fn seek_to(&mut self, _position_ms: u64) -> PlayerResult<()> {
        Ok(())
    }

    fn set_gain(&mut self, _gain: domain::GainDecision) -> PlayerResult<()> {
        Ok(())
    }

    fn position_ms(&self) -> PlayerResult<u64> {
        Ok(250)
    }
}

mod library;
mod playback;
mod playlists;
mod tracks;
mod user_activity;

fn create_app(db_path: &Path, media_root: &Path) -> *mut PlayerApp {
    let db_path = c_string_arg(db_path);
    let media_root = c_string_arg(media_root);
    let app = unsafe { player_app_create(db_path.as_ptr(), media_root.as_ptr()) };
    assert!(!app.is_null());
    app
}

unsafe fn call_json(response: *mut c_char) -> Value {
    assert!(!response.is_null());
    let text = CStr::from_ptr(response).to_string_lossy().into_owned();
    player_string_free(response);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"))
}

fn assert_ok(response: &Value) {
    assert_eq!(response["ok"], true, "{response}");
}

fn playlist_paths(response: &Value) -> Vec<String> {
    response["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["path"].as_str().unwrap().to_owned())
        .collect()
}

fn queue_paths(response: &Value) -> Vec<String> {
    response["data"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["path"].as_str().unwrap().to_owned())
        .collect()
}

fn c_string_arg(value: impl AsRef<Path>) -> CString {
    CString::new(value.as_ref().to_string_lossy().into_owned()).unwrap()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("ffi_{prefix}_{nonce}.sqlite3"))
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("ffi_{prefix}_{nonce}"))
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

    let mut file = fs::File::create(path)?;
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
