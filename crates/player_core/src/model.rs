use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrackId(u64);

impl TrackId {
    pub fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub fn from_path(path: &Path) -> Self {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        Self(hasher.finish())
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TrackViewId(String);

impl TrackViewId {
    pub fn from_value(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn primary_from_audio_hash(audio_hash: &str) -> Self {
        Self(format!("audio:{}", audio_hash.trim()))
    }

    pub fn fallback_from_path(path: &Path) -> Self {
        Self(format!("path:{:016x}", TrackId::from_path(path).value()))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackViewKind {
    Primary,
    Derived,
}

impl TrackViewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Derived => "derived",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "derived" => Self::Derived,
            _ => Self::Primary,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: TrackId,
    pub view_id: TrackViewId,
    pub primary_view_id: TrackViewId,
    pub view_kind: TrackViewKind,
    pub transform_spec: Option<String>,
    pub quality_profile: Option<String>,
    pub format_name: Option<String>,
    pub view_name: Option<String>,
    pub user_rating: Option<u8>,
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub duration_ms: Option<u64>,
    pub artwork_count: u32,
    pub fingerprint: Option<FileFingerprint>,
    pub file_hash: Option<String>,
    pub audio_hash: Option<String>,
    pub loudness: Option<LoudnessInfo>,
}

impl Track {
    pub fn from_path(path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Untitled")
            .to_owned();
        let view_id = TrackViewId::fallback_from_path(&path);

        Self {
            id: TrackId::from_path(&path),
            view_id: view_id.clone(),
            primary_view_id: view_id,
            view_kind: TrackViewKind::Primary,
            transform_spec: None,
            quality_profile: None,
            format_name: path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase()),
            view_name: None,
            user_rating: None,
            path,
            title,
            artist: None,
            album: None,
            album_artist: None,
            genre: None,
            track_number: None,
            disc_number: None,
            year: None,
            duration_ms: None,
            artwork_count: 0,
            fingerprint: None,
            file_hash: None,
            audio_hash: None,
            loudness: None,
        }
    }

    pub fn set_primary_audio_hash(&mut self, audio_hash: impl Into<String>) {
        let audio_hash = audio_hash.into();
        let view_id = TrackViewId::primary_from_audio_hash(&audio_hash);
        self.audio_hash = Some(audio_hash);
        self.view_id = view_id.clone();
        self.primary_view_id = view_id;
        self.view_kind = TrackViewKind::Primary;
        self.transform_spec = None;
    }

    pub fn apply_metadata(&mut self, metadata: TrackMetadata) {
        if let Some(title) = metadata.title.filter(|title| !title.trim().is_empty()) {
            self.title = title;
        }
        self.artist = metadata.artist;
        self.album = metadata.album;
        self.album_artist = metadata.album_artist;
        self.genre = metadata.genre;
        self.track_number = metadata.track_number;
        self.disc_number = metadata.disc_number;
        self.year = metadata.year;
        self.duration_ms = metadata.duration_ms;
        self.artwork_count = metadata.artwork_count;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub duration_ms: Option<u64>,
    pub artwork_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtworkImage {
    pub picture_index: u32,
    pub mime_type: Option<String>,
    pub picture_type: String,
    pub description: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileFingerprint {
    pub size_bytes: u64,
    pub modified_unix_seconds: i64,
}

impl FileFingerprint {
    pub fn new(size_bytes: u64, modified_unix_seconds: i64) -> Self {
        Self {
            size_bytes,
            modified_unix_seconds,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoudnessInfo {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub album_integrated_lufs: Option<f32>,
    pub album_true_peak_dbtp: Option<f32>,
    pub analysis_version: u32,
}

impl LoudnessInfo {
    pub fn track(integrated_lufs: f32, true_peak_dbtp: f32) -> Self {
        Self {
            integrated_lufs,
            true_peak_dbtp,
            album_integrated_lufs: None,
            album_true_peak_dbtp: None,
            analysis_version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_from_path_uses_file_stem_as_title() {
        let track = Track::from_path("/music/Artist/Song Title.flac".into());
        assert_eq!(track.title, "Song Title");
        assert_eq!(track.artist, None);
        assert_eq!(track.album, None);
        assert_eq!(track.album_artist, None);
        assert_eq!(track.fingerprint, None);
    }

    #[test]
    fn track_id_is_stable_for_same_path() {
        let left = TrackId::from_path(Path::new("/music/song.mp3"));
        let right = TrackId::from_path(Path::new("/music/song.mp3"));
        let other = TrackId::from_path(Path::new("/music/other.mp3"));

        assert_eq!(left, right);
        assert_ne!(left, other);
    }

    #[test]
    fn metadata_can_override_file_stem_fields() {
        let mut track = Track::from_path("/music/raw-name.ogg".into());
        track.apply_metadata(TrackMetadata {
            title: Some("Actual Title".to_owned()),
            artist: Some("Artist".to_owned()),
            album: Some("Album".to_owned()),
            album_artist: Some("Album Artist".to_owned()),
            genre: Some("Electronic".to_owned()),
            track_number: Some(7),
            disc_number: Some(2),
            year: Some(2026),
            duration_ms: Some(1234),
            artwork_count: 1,
        });

        assert_eq!(track.title, "Actual Title");
        assert_eq!(track.artist.as_deref(), Some("Artist"));
        assert_eq!(track.album.as_deref(), Some("Album"));
        assert_eq!(track.album_artist.as_deref(), Some("Album Artist"));
        assert_eq!(track.genre.as_deref(), Some("Electronic"));
        assert_eq!(track.track_number, Some(7));
        assert_eq!(track.disc_number, Some(2));
        assert_eq!(track.year, Some(2026));
        assert_eq!(track.duration_ms, Some(1234));
        assert_eq!(track.artwork_count, 1);
    }

    #[test]
    fn artwork_image_keeps_binary_payload() {
        let image = ArtworkImage {
            picture_index: 0,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: Some("front".to_owned()),
            data: vec![1, 2, 3],
        };

        assert_eq!(image.data.len(), 3);
        assert_eq!(image.mime_type.as_deref(), Some("image/png"));
    }
}
