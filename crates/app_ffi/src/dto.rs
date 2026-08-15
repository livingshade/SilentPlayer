use std::path::{Path, PathBuf};

use domain::{gain_for_track, LoudnessStatus, NormalizationSettings, Track};
use errors::{PlayerError, PlayerResult};
use library_service::{LyricsDocument, TimedLyricsLine};
use serde::{Deserialize, Serialize};
use store_sqlite::LibraryStore;

use crate::file_support::resolved_artwork_path;
use crate::support::path_to_string_lossy;

#[derive(Serialize)]
pub(super) struct Response<T: Serialize> {
    pub(super) ok: bool,
    pub(super) data: Option<T>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TrackDto {
    pub(super) id: String,
    pub(super) view_id: String,
    pub(super) primary_view_id: String,
    pub(super) is_primary_view: bool,
    pub(super) view_kind: String,
    pub(super) view_name: Option<String>,
    pub(super) rating: Option<u8>,
    pub(super) title: String,
    pub(super) artist: Option<String>,
    pub(super) album: Option<String>,
    pub(super) duration_ms: Option<u64>,
    pub(super) artwork_count: u32,
    pub(super) artwork_path: Option<String>,
    pub(super) artwork_source: Option<String>,
    pub(super) has_album_identity: bool,
    pub(super) path: String,
    pub(super) quality_profile: Option<String>,
    pub(super) format_name: Option<String>,
    pub(super) gain_db: Option<f32>,
    pub(super) loudness_status: String,
}

#[derive(Serialize)]
pub(super) struct ImportSummary {
    pub(super) imported: usize,
    pub(super) copied: usize,
    pub(super) duplicates_skipped: usize,
    pub(super) artwork_cached: usize,
    pub(super) metadata_warnings: usize,
    pub(super) failures: usize,
}

#[derive(Serialize)]
pub(super) struct DeleteTrackSummary {
    pub(super) managed_files_deleted: usize,
    pub(super) cleanup_error: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct LibraryPackageManifest {
    pub(super) format_version: u32,
    pub(super) database_file: String,
    pub(super) tracks: Vec<LibraryPackageTrack>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct LibraryPackageTrack {
    pub(super) database_path: String,
    pub(super) audio_file: String,
}

#[derive(Serialize)]
pub(super) struct LibraryPackageSummary {
    pub(super) tracks: usize,
    pub(super) playlists: usize,
    pub(super) audio_files: usize,
    pub(super) sidecar_files: usize,
}

#[derive(Serialize)]
pub(super) struct AnalysisSummary {
    pub(super) tracks_analyzed: usize,
    pub(super) track_failures: usize,
    pub(super) albums_analyzed: usize,
    pub(super) album_tracks_updated: usize,
    pub(super) album_skipped: usize,
}

#[derive(Serialize)]
pub(super) struct PlaybackSnapshot {
    pub(super) is_playing: bool,
    pub(super) position_ms: u64,
    pub(super) current_track: Option<TrackDto>,
    pub(super) queue_len: usize,
    pub(super) queue_position: Option<usize>,
    pub(super) playback_mode: String,
    pub(super) repeat_mode: String,
    pub(super) shuffle_enabled: bool,
    pub(super) gain_db: Option<f32>,
    pub(super) loudness_status: Option<String>,
    pub(super) error: Option<String>,
    pub(super) interruption_active: bool,
    pub(super) resume_after_interruption: bool,
}

#[derive(Serialize)]
pub(super) struct PlaybackQueueDto {
    pub(super) tracks: Vec<TrackDto>,
    pub(super) current_index: Option<usize>,
    pub(super) playback_mode: String,
    pub(super) repeat_mode: String,
    pub(super) shuffle_enabled: bool,
}

#[derive(Serialize)]
pub(super) struct LibraryPageDto {
    pub(super) total: usize,
    pub(super) offset: usize,
    pub(super) tracks: Vec<TrackDto>,
}

#[derive(Serialize)]
pub(super) struct TrackDetailsDto {
    pub(super) view_id: String,
    pub(super) primary_view_id: String,
    pub(super) is_primary_view: bool,
    pub(super) view_kind: String,
    pub(super) view_name: Option<String>,
    pub(super) rating: Option<u8>,
    pub(super) transform_spec: Option<String>,
    pub(super) quality_profile: Option<String>,
    pub(super) format_name: Option<String>,
    pub(super) artwork_path: Option<String>,
    pub(super) artwork_source: Option<String>,
    pub(super) lyrics_path: Option<String>,
    pub(super) lyrics_text: Option<String>,
    pub(super) lyrics_document: LyricsDocument,
    pub(super) notes: Option<String>,
    pub(super) audio_hash: String,
    pub(super) original_title: String,
    pub(super) original_artist: Option<String>,
    pub(super) original_album: Option<String>,
    pub(super) display_title: String,
    pub(super) display_artist: Option<String>,
    pub(super) display_album: Option<String>,
    pub(super) play_count: u64,
    pub(super) playback_session_count: u64,
    pub(super) last_played_at_unix_seconds: Option<i64>,
    pub(super) last_completed_at_unix_seconds: Option<i64>,
}

#[derive(Serialize)]
pub(super) struct TrackLyricsDto {
    pub(super) view_id: String,
    pub(super) lyrics_path: Option<String>,
    pub(super) lyrics_text: Option<String>,
    pub(super) lyrics_document: LyricsDocument,
}

#[derive(Serialize)]
pub(super) struct TrackLyricsAtDto {
    pub(super) view_id: String,
    pub(super) position_ms: u64,
    pub(super) kind: String,
    pub(super) line_index: Option<usize>,
    pub(super) line: Option<TimedLyricsLine>,
    pub(super) previous_index: Option<usize>,
    pub(super) next_index: Option<usize>,
    pub(super) display_text: String,
    pub(super) is_instrumental: bool,
}

#[derive(Serialize)]
pub(super) struct TrackLyricsRemovalDto {
    pub(super) view_id: String,
    pub(super) files_removed: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrackViewEditRequest {
    pub(super) title: String,
    pub(super) artist: Option<String>,
    pub(super) album: Option<String>,
    pub(super) notes: Option<String>,
    pub(super) artwork_path: Option<String>,
    pub(super) lyrics_path: Option<String>,
}

#[derive(Serialize)]
pub(super) struct AlbumArtworkSummary {
    pub(super) tracks_updated: usize,
}

#[derive(Serialize)]
pub(super) struct AuditSummary {
    pub(super) tracks_scanned: usize,
    pub(super) hashes_updated: usize,
    pub(super) duplicate_groups: usize,
    pub(super) tracks_merged: usize,
    pub(super) failures: usize,
}

#[derive(Serialize)]
pub(super) struct PlaylistDto {
    pub(super) id: i64,
    pub(super) name: String,
    pub(super) track_count: usize,
    pub(super) artwork_path: Option<String>,
    pub(super) artwork_source: Option<String>,
    pub(super) created_at_unix_seconds: i64,
    pub(super) updated_at_unix_seconds: i64,
    pub(super) last_used_at_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub(super) struct UserActivityStore {
    pub(super) root: PathBuf,
    pub(super) profile_path: PathBuf,
    pub(super) history_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct LocalUserProfile {
    pub(super) schema_version: u32,
    pub(super) user_id: String,
    pub(super) display_name: String,
    pub(super) sync_enabled: bool,
    pub(super) created_at_unix_seconds: i64,
    pub(super) updated_at_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub(super) struct ActivePlaybackSession {
    pub(super) session_id: String,
    pub(super) track: TrackDto,
    pub(super) started_at_unix_seconds: i64,
    pub(super) start_position_ms: u64,
    pub(super) last_position_ms: u64,
    pub(super) listened_ms: u64,
    pub(super) seek_count: u32,
}

#[derive(Serialize)]
pub(super) struct UserDataDto {
    pub(super) user_id: String,
    pub(super) display_name: String,
    pub(super) sync_enabled: bool,
    pub(super) profile_path: String,
    pub(super) history_path: String,
    pub(super) created_at_unix_seconds: i64,
}

#[derive(Serialize)]
pub(super) struct PlaybackHistoryRecord {
    pub(super) schema_version: u32,
    pub(super) record_type: String,
    pub(super) user_id: String,
    pub(super) session_id: String,
    pub(super) started_at_unix_seconds: i64,
    pub(super) ended_at_unix_seconds: i64,
    pub(super) start_position_ms: u64,
    pub(super) end_position_ms: u64,
    pub(super) listened_ms: u64,
    pub(super) track_duration_ms: Option<u64>,
    pub(super) completion_ratio: Option<f32>,
    pub(super) completed: bool,
    pub(super) finish_reason: String,
    pub(super) seek_count: u32,
    pub(super) track: PlaybackTrackRecord,
}

#[derive(Serialize)]
pub(super) struct PlaybackTrackRecord {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) artist: Option<String>,
    pub(super) album: Option<String>,
    pub(super) path: String,
    pub(super) gain_db: Option<f32>,
    pub(super) loudness_status: String,
}

#[derive(Serialize)]
pub(super) struct Empty {}

pub(super) fn import_summary_dto(summary: library_service::ImportSummary) -> ImportSummary {
    ImportSummary {
        imported: summary.imported,
        copied: summary.copied,
        duplicates_skipped: summary.duplicates_skipped,
        artwork_cached: summary.artwork_cached,
        metadata_warnings: summary.metadata_warnings,
        failures: summary.failures,
    }
}

#[cfg(test)]
pub(super) fn track_dtos(tracks: &[Track]) -> PlayerResult<Vec<TrackDto>> {
    tracks.iter().map(track_to_dto).collect()
}

pub(super) fn track_to_dto(track: &Track) -> PlayerResult<TrackDto> {
    let gain = gain_for_track(track, NormalizationSettings::default());
    let view_id = track_view_id(track)?;
    Ok(TrackDto {
        id: view_id.to_owned(),
        view_id: view_id.to_owned(),
        primary_view_id: track.primary_view_id.value().to_owned(),
        is_primary_view: track.view_id == track.primary_view_id,
        view_kind: track.view_kind.as_str().to_owned(),
        view_name: track.view_name.clone(),
        rating: track.user_rating,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration_ms: track.duration_ms,
        artwork_count: track.artwork_count,
        artwork_path: None,
        artwork_source: None,
        has_album_identity: track_has_album_identity(track),
        path: track.path.to_string_lossy().into_owned(),
        quality_profile: track.quality_profile.clone(),
        format_name: track.format_name.clone(),
        gain_db: if gain.status == LoudnessStatus::Ready {
            Some(gain.gain_db)
        } else {
            None
        },
        loudness_status: format!("{:?}", gain.status),
    })
}

pub(super) fn track_dtos_with_artwork(
    tracks: &[Track],
    store: &LibraryStore,
    db_path: &Path,
) -> PlayerResult<Vec<TrackDto>> {
    tracks
        .iter()
        .map(|track| track_to_dto_with_artwork(track, store, db_path))
        .collect()
}

pub(super) fn track_to_dto_with_artwork(
    track: &Track,
    store: &LibraryStore,
    db_path: &Path,
) -> PlayerResult<TrackDto> {
    let mut dto = track_to_dto(track)?;
    if let Some((path, source)) = resolved_artwork_path(store, db_path, &track.path)? {
        dto.artwork_path = Some(path_to_string_lossy(&path));
        dto.artwork_source = Some(source.to_owned());
    }
    Ok(dto)
}

pub(super) fn track_has_album_identity(track: &Track) -> bool {
    track
        .album
        .as_deref()
        .is_some_and(|album| !album.trim().is_empty())
}

pub(super) fn track_view_id(track: &Track) -> PlayerResult<&str> {
    let view_id = track.view_id.value();
    if view_id.trim().is_empty() {
        return Err(PlayerError::store(format!(
            "track is missing view id: {}",
            track.path.display()
        )));
    }
    Ok(view_id)
}

pub(super) fn required_audio_hash(audio_hash: Option<String>, path: &Path) -> PlayerResult<String> {
    audio_hash
        .filter(|hash| !hash.trim().is_empty())
        .ok_or_else(|| {
            PlayerError::store(format!("track is missing audio hash: {}", path.display()))
        })
}

pub(super) fn cache_key_for_view_id(view_id: &str) -> String {
    view_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}
