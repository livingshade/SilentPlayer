use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use player_core::{FileFingerprint, Track};
use player_error::{PlayerError, PlayerResult};

pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "alac", "flac", "m4a", "mp3", "ogg", "opus", "wav",
];

#[derive(Clone, Debug, Default)]
pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub include_hidden: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryScanner {
    options: ScanOptions,
}

impl LibraryScanner {
    pub fn new(options: ScanOptions) -> Self {
        Self { options }
    }

    pub fn scan(&self, root: impl AsRef<Path>) -> PlayerResult<Vec<Track>> {
        let root = root.as_ref();
        let mut tracks = Vec::new();
        self.scan_dir(root, &mut tracks)?;
        tracks.sort_by(|left, right| {
            left.title
                .to_lowercase()
                .cmp(&right.title.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(tracks)
    }

    fn scan_dir(&self, dir: &Path, tracks: &mut Vec<Track>) -> PlayerResult<()> {
        let entries =
            fs::read_dir(dir).map_err(|source| PlayerError::io(dir.to_path_buf(), source))?;

        for entry in entries {
            let entry = entry.map_err(|source| PlayerError::io(dir.to_path_buf(), source))?;
            let path = entry.path();
            let file_name = entry.file_name();

            if !self.options.include_hidden && is_hidden_name(&file_name) {
                continue;
            }

            let file_type = if self.options.follow_symlinks {
                fs::metadata(&path).map(|metadata| metadata.file_type())
            } else {
                entry.file_type()
            }
            .map_err(|source| PlayerError::io(path.clone(), source))?;

            if file_type.is_dir() {
                self.scan_dir(&path, tracks)?;
            } else if file_type.is_file() && is_supported_audio_file(&path) {
                let fingerprint = fs::metadata(&path)
                    .ok()
                    .map(|metadata| fingerprint_from_metadata(&metadata));
                let mut track = Track::from_path(path);
                track.fingerprint = fingerprint;
                tracks.push(track);
            }
        }

        Ok(())
    }
}

pub fn fingerprint_from_metadata(metadata: &fs::Metadata) -> FileFingerprint {
    let modified_unix_seconds = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0);
    FileFingerprint::new(metadata.len(), modified_unix_seconds)
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_AUDIO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

fn is_hidden_name(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_supported_extensions_case_insensitively() {
        assert!(is_supported_audio_file(Path::new("song.MP3")));
        assert!(is_supported_audio_file(Path::new("song.flac")));
        assert!(!is_supported_audio_file(Path::new("cover.jpg")));
    }

    #[test]
    fn scans_recursively_and_skips_hidden_by_default() {
        let root = make_temp_dir("scan_default");
        fs::create_dir(root.join("nested")).unwrap();
        fs::create_dir(root.join(".hidden")).unwrap();
        fs::write(root.join("b.FLAC"), []).unwrap();
        fs::write(root.join("nested").join("a.mp3"), []).unwrap();
        fs::write(root.join(".hidden").join("secret.mp3"), []).unwrap();
        fs::write(root.join("cover.jpg"), []).unwrap();

        let tracks = LibraryScanner::default().scan(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();

        let titles: Vec<_> = tracks.iter().map(|track| track.title.as_str()).collect();
        assert_eq!(titles, vec!["a", "b"]);
    }

    #[test]
    fn can_include_hidden_entries() {
        let root = make_temp_dir("scan_hidden");
        fs::create_dir(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden").join("secret.mp3"), []).unwrap();

        let scanner = LibraryScanner::new(ScanOptions {
            include_hidden: true,
            follow_symlinks: false,
        });
        let tracks = scanner.scan(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "secret");
    }

    fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("player_library_fs_{prefix}_{nonce}"));
        fs::create_dir(&path).unwrap();
        path
    }
}
