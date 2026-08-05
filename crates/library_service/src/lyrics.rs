use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};
use serde::Serialize;

use crate::LYRICS_EXTENSIONS;

pub const INSTRUMENTAL_LYRICS_TOKEN: &str = "♪";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsFormat {
    Lrc,
    PlainText,
    Instrumental,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LyricsDocument {
    pub format: LyricsFormat,
    pub instrumental_token: String,
    pub metadata: LyricsMetadata,
    pub content: LyricsContent,
    pub diagnostics: Vec<LyricsDiagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LyricsDisplay<'a> {
    Lyric(&'a str),
    Instrumental(&'a str),
}

impl<'a> LyricsDisplay<'a> {
    pub fn display_text(self) -> &'a str {
        match self {
            Self::Lyric(text) | Self::Instrumental(text) => text,
        }
    }

    pub fn is_instrumental(self) -> bool {
        matches!(self, Self::Instrumental(_))
    }
}

impl LyricsDocument {
    pub fn instrumental() -> Self {
        Self {
            format: LyricsFormat::Instrumental,
            instrumental_token: INSTRUMENTAL_LYRICS_TOKEN.to_owned(),
            metadata: LyricsMetadata::default(),
            content: LyricsContent::Instrumental,
            diagnostics: Vec::new(),
        }
    }

    pub fn timed_lines(&self) -> Option<&[TimedLyricsLine]> {
        match &self.content {
            LyricsContent::Timed { lines } => Some(lines),
            LyricsContent::Plain { .. } | LyricsContent::Instrumental => None,
        }
    }

    pub fn active_line_index(&self, position_ms: u64) -> Option<usize> {
        let lines = self.timed_lines()?;
        let insertion = lines.partition_point(|line| line.start_ms <= position_ms);
        insertion.checked_sub(1)
    }

    pub fn active_line(&self, position_ms: u64) -> Option<&TimedLyricsLine> {
        self.active_line_index(position_ms)
            .and_then(|index| self.timed_lines()?.get(index))
    }

    pub fn display_at(&self, position_ms: u64) -> LyricsDisplay<'_> {
        let text = match &self.content {
            LyricsContent::Timed { .. } => self
                .active_line(position_ms)
                .map(|line| line.text.trim())
                .filter(|text| !text.is_empty()),
            // Plain lyrics are useful in the full lyrics view, but they do not
            // describe coverage for any playback position. Treat the compact
            // playback display as instrumental until synchronized timestamps
            // are available instead of pinning the first/static text forever.
            LyricsContent::Plain { .. } => None,
            LyricsContent::Instrumental => None,
        };
        text.map(LyricsDisplay::Lyric)
            .unwrap_or_else(|| LyricsDisplay::Instrumental(&self.instrumental_token))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LyricsContent {
    Timed { lines: Vec<TimedLyricsLine> },
    Plain { text: String },
    Instrumental,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TimedLyricsLine {
    pub id: u32,
    pub start_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LyricsMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub author: Option<String>,
    pub offset_ms: i64,
    pub tags: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsDiagnosticSeverity {
    Info,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LyricsDiagnostic {
    pub severity: LyricsDiagnosticSeverity,
    pub code: String,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LyricsAsset {
    pub path: PathBuf,
    pub raw_text: String,
    pub document: LyricsDocument,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LyricsRemoval {
    pub files_removed: usize,
}

#[derive(Debug)]
struct RawTimedLine {
    start_ms: u64,
    source_order: usize,
    text: String,
}

pub fn parse_lyrics_text(text: &str, format: LyricsFormat) -> PlayerResult<LyricsDocument> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match format {
        LyricsFormat::PlainText => {
            if contains_lrc_timestamp(text)? {
                parse_lrc(text)
            } else {
                Ok(LyricsDocument {
                    format,
                    instrumental_token: INSTRUMENTAL_LYRICS_TOKEN.to_owned(),
                    metadata: LyricsMetadata::default(),
                    content: LyricsContent::Plain {
                        text: text.to_owned(),
                    },
                    diagnostics: Vec::new(),
                })
            }
        }
        LyricsFormat::Lrc => parse_lrc(text),
        LyricsFormat::Instrumental => Err(PlayerError::invalid_input(
            "instrumental lyrics are a virtual display state, not a file format",
        )),
    }
}

pub fn load_lyrics_file(path: &Path) -> PlayerResult<LyricsAsset> {
    let format = format_for_path(path)?;
    let bytes = fs::read(path).map_err(|source| PlayerError::io(path, source))?;
    let raw_text = String::from_utf8(bytes).map_err(|error| {
        PlayerError::invalid_input(format!(
            "lyrics file {} must be UTF-8: {error}",
            path.display()
        ))
    })?;
    let raw_text = raw_text
        .strip_prefix('\u{feff}')
        .unwrap_or(&raw_text)
        .to_owned();
    let document = parse_lyrics_text(&raw_text, format)?;
    Ok(LyricsAsset {
        path: path.to_path_buf(),
        raw_text,
        document,
    })
}

pub fn load_track_lyrics(track_path: &Path) -> PlayerResult<Option<LyricsAsset>> {
    let Some(path) = lyrics_sidecar_path(track_path) else {
        return Ok(None);
    };
    load_lyrics_file(&path).map(Some)
}

pub fn set_track_lyrics(track_path: &Path, source_path: &Path) -> PlayerResult<LyricsAsset> {
    let source = load_lyrics_file(source_path)?;
    let destination_extension = match source.document.content {
        LyricsContent::Timed { .. } => "lrc",
        LyricsContent::Plain { .. } => "txt",
        LyricsContent::Instrumental => {
            return Err(PlayerError::invalid_input(
                "instrumental lyrics cannot be stored as a lyrics file",
            ));
        }
    };
    let destination = track_sidecar_path(track_path, destination_extension)?;
    let source_canonical = source_path.canonicalize().ok();
    let destination_canonical = destination.canonicalize().ok();

    if source_canonical.is_none() || source_canonical != destination_canonical {
        fs::copy(source_path, &destination)
            .map_err(|source| PlayerError::io(destination.clone(), source))?;
    }

    for path in track_lyrics_paths(track_path) {
        let is_destination = path == destination
            || destination_canonical.is_some() && path.canonicalize().ok() == destination_canonical;
        if !is_destination {
            fs::remove_file(&path).map_err(|source| PlayerError::io(path, source))?;
        }
    }

    load_lyrics_file(&destination)
}

pub fn remove_track_lyrics(track_path: &Path) -> PlayerResult<LyricsRemoval> {
    let mut files_removed = 0;
    for path in track_lyrics_paths(track_path) {
        fs::remove_file(&path).map_err(|source| PlayerError::io(path, source))?;
        files_removed += 1;
    }
    Ok(LyricsRemoval { files_removed })
}

fn parse_lrc(text: &str) -> PlayerResult<LyricsDocument> {
    let mut metadata = LyricsMetadata::default();
    let mut diagnostics = Vec::new();
    let mut timed = Vec::new();
    let mut untimed = Vec::new();
    let mut source_order = 0_usize;

    for (line_offset, original_line) in text.lines().enumerate() {
        let line_number = line_offset + 1;
        let line = original_line.trim_end_matches('\r');
        let mut remainder = line.trim_start();
        let mut timestamps = Vec::new();
        let mut consumed_metadata = false;

        while let Some(after_open) = remainder.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                break;
            };
            let token = &after_open[..close];
            let after_tag = &after_open[close + 1..];
            match parse_timestamp(token) {
                Ok(Some(timestamp)) => {
                    timestamps.push(timestamp);
                    remainder = after_tag;
                }
                Ok(None) => {
                    if looks_like_timestamp(token) {
                        return Err(PlayerError::invalid_input(format!(
                            "invalid LRC timestamp `{token}` at line {line_number}"
                        )));
                    }
                    if timestamps.is_empty()
                        && apply_metadata_tag(token, line_number, &mut metadata, &mut diagnostics)?
                    {
                        consumed_metadata = true;
                        remainder = after_tag;
                    } else {
                        break;
                    }
                }
                Err(message) => {
                    return Err(PlayerError::invalid_input(format!(
                        "{message} at line {line_number}"
                    )));
                }
            }
        }

        if !timestamps.is_empty() {
            let lyric = remainder.trim_start().to_owned();
            for timestamp in timestamps {
                timed.push(RawTimedLine {
                    start_ms: timestamp,
                    source_order,
                    text: lyric.clone(),
                });
                source_order += 1;
            }
        } else if !consumed_metadata && !line.trim().is_empty() {
            untimed.push(line.to_owned());
        }
    }

    if timed.is_empty() {
        if !untimed.is_empty() {
            diagnostics.push(LyricsDiagnostic {
                severity: LyricsDiagnosticSeverity::Info,
                code: "no_timestamps".to_owned(),
                line: None,
                message:
                    "No synchronized timestamps were found; lyrics will display as plain text."
                        .to_owned(),
            });
        }
        return Ok(LyricsDocument {
            format: LyricsFormat::Lrc,
            instrumental_token: INSTRUMENTAL_LYRICS_TOKEN.to_owned(),
            metadata,
            content: LyricsContent::Plain {
                text: untimed.join("\n"),
            },
            diagnostics,
        });
    }

    if !untimed.is_empty() {
        diagnostics.push(LyricsDiagnostic {
            severity: LyricsDiagnosticSeverity::Warning,
            code: "untimed_lines_ignored".to_owned(),
            line: None,
            message: format!(
                "{} non-empty line(s) without timestamps are excluded from the synchronized timeline.",
                untimed.len()
            ),
        });
    }

    let offset = i128::from(metadata.offset_ms);
    let mut clamped = 0_usize;
    let mut lines = Vec::with_capacity(timed.len());
    for raw in timed {
        let effective = i128::from(raw.start_ms) + offset;
        let start_ms = if effective < 0 {
            clamped += 1;
            0
        } else {
            u64::try_from(effective).map_err(|_| {
                PlayerError::invalid_input("LRC timestamp overflows after applying offset")
            })?
        };
        lines.push((start_ms, raw.source_order, raw.text));
    }
    if clamped > 0 {
        diagnostics.push(LyricsDiagnostic {
            severity: LyricsDiagnosticSeverity::Warning,
            code: "negative_timestamp_clamped".to_owned(),
            line: None,
            message: format!(
                "{clamped} timestamp(s) became negative after applying offset and were clamped to zero."
            ),
        });
    }

    lines.sort_by_key(|(start_ms, source_order, _)| (*start_ms, *source_order));
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(id, (start_ms, _, text))| {
            let id = u32::try_from(id)
                .map_err(|_| PlayerError::invalid_input("lyrics contain too many timed lines"))?;
            Ok(TimedLyricsLine { id, start_ms, text })
        })
        .collect::<PlayerResult<Vec<_>>>()?;

    Ok(LyricsDocument {
        format: LyricsFormat::Lrc,
        instrumental_token: INSTRUMENTAL_LYRICS_TOKEN.to_owned(),
        metadata,
        content: LyricsContent::Timed { lines },
        diagnostics,
    })
}

fn contains_lrc_timestamp(text: &str) -> PlayerResult<bool> {
    for (line_offset, line) in text.lines().enumerate() {
        let mut remainder = line.trim_start();
        while let Some(after_open) = remainder.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                if looks_like_timestamp(after_open) {
                    return Err(PlayerError::invalid_input(format!(
                        "unterminated LRC timestamp at line {}",
                        line_offset + 1
                    )));
                }
                break;
            };
            let token = &after_open[..close];
            match parse_timestamp(token) {
                Ok(Some(_)) => return Ok(true),
                Ok(None) if looks_like_timestamp(token) => {
                    return Err(PlayerError::invalid_input(format!(
                        "invalid LRC timestamp `{token}` at line {}",
                        line_offset + 1
                    )));
                }
                Ok(None) => remainder = &after_open[close + 1..],
                Err(message) => {
                    return Err(PlayerError::invalid_input(format!(
                        "{message} at line {}",
                        line_offset + 1
                    )));
                }
            }
        }
    }
    Ok(false)
}

fn parse_timestamp(token: &str) -> Result<Option<u64>, String> {
    let Some((minutes, seconds)) = token.split_once(':') else {
        return Ok(None);
    };
    if minutes.is_empty() || !minutes.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    let (seconds, fraction) = match seconds.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (seconds, None),
    };
    if seconds.is_empty() || seconds.len() > 2 || !seconds.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Ok(None);
    }
    let minutes = minutes
        .parse::<u64>()
        .map_err(|_| format!("invalid LRC minutes `{minutes}`"))?;
    let seconds_value = seconds
        .parse::<u64>()
        .map_err(|_| format!("invalid LRC seconds `{seconds}`"))?;
    if seconds_value >= 60 {
        return Err(format!("LRC seconds must be below 60 in `{token}`"));
    }
    let fraction_ms = match fraction {
        None => 0,
        Some(value)
            if !value.is_empty()
                && value.len() <= 3
                && value.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let parsed = value
                .parse::<u64>()
                .map_err(|_| format!("invalid LRC fraction `{value}`"))?;
            parsed * 10_u64.pow(3 - value.len() as u32)
        }
        Some(_) => return Ok(None),
    };
    let start_ms = minutes
        .checked_mul(60_000)
        .and_then(|value| value.checked_add(seconds_value * 1_000))
        .and_then(|value| value.checked_add(fraction_ms))
        .ok_or_else(|| format!("LRC timestamp `{token}` overflows"))?;
    Ok(Some(start_ms))
}

fn looks_like_timestamp(token: &str) -> bool {
    token
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit())
        && token.contains(':')
}

fn apply_metadata_tag(
    token: &str,
    line_number: usize,
    metadata: &mut LyricsMetadata,
    diagnostics: &mut Vec<LyricsDiagnostic>,
) -> PlayerResult<bool> {
    let Some((key, value)) = token.split_once(':') else {
        return Ok(false);
    };
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Ok(false);
    }
    let key = key.to_ascii_lowercase();
    let value = value.trim().to_owned();
    metadata
        .tags
        .entry(key.clone())
        .or_default()
        .push(value.clone());
    match key.as_str() {
        "ti" => metadata.title = non_empty(value),
        "ar" => metadata.artist = non_empty(value),
        "al" => metadata.album = non_empty(value),
        "by" => metadata.author = non_empty(value),
        "offset" => {
            metadata.offset_ms = value.parse::<i64>().map_err(|_| {
                PlayerError::invalid_input(format!(
                    "invalid LRC offset `{value}` at line {line_number}"
                ))
            })?;
        }
        _ => diagnostics.push(LyricsDiagnostic {
            severity: LyricsDiagnosticSeverity::Info,
            code: "unknown_metadata".to_owned(),
            line: Some(line_number),
            message: format!("Preserved unrecognized LRC metadata tag `{key}`."),
        }),
    }
    Ok(true)
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn format_for_path(path: &Path) -> PlayerResult<LyricsFormat> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlayerError::invalid_input("lyrics file must have an extension"))?;
    if extension.eq_ignore_ascii_case("txt") {
        return Ok(LyricsFormat::PlainText);
    }
    if extension.eq_ignore_ascii_case("lrc") || extension.eq_ignore_ascii_case("lyrics") {
        return Ok(LyricsFormat::Lrc);
    }
    Err(PlayerError::invalid_input(format!(
        "unsupported lyrics extension `.{extension}`; expected .lrc, .txt, or .lyrics"
    )))
}

fn track_sidecar_path(track_path: &Path, extension: &str) -> PlayerResult<PathBuf> {
    let dir = track_path.parent().ok_or_else(|| {
        PlayerError::metadata(format!(
            "track has no parent directory: {}",
            track_path.display()
        ))
    })?;
    let stem = track_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlayerError::metadata("track has no UTF-8 file stem"))?;
    Ok(dir.join(format!("{stem}.{extension}")))
}

fn lyrics_sidecar_path(track_path: &Path) -> Option<PathBuf> {
    for extension in LYRICS_EXTENSIONS {
        let path = track_sidecar_path(track_path, extension).ok()?;
        if path.is_file() {
            return Some(path);
        }
    }
    track_lyrics_paths(track_path).into_iter().next()
}

fn track_lyrics_paths(track_path: &Path) -> Vec<PathBuf> {
    let Some(dir) = track_path.parent() else {
        return Vec::new();
    };
    let Some(stem) = track_path.file_stem().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(stem))
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| {
                        LYRICS_EXTENSIONS
                            .iter()
                            .any(|supported| extension.eq_ignore_ascii_case(supported))
                    })
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn parses_timestamps_metadata_multiple_tags_and_offset() {
        let document = parse_lyrics_text(
            "\u{feff}[ar:Artist]\r\n[ti:Song]\r\n[offset:+200]\r\n[00:12]One\r\n[00:15.4]Two\r\n[00:18.42][01:00.420]Chorus\r\n",
            LyricsFormat::Lrc,
        )
        .unwrap();

        assert_eq!(document.metadata.artist.as_deref(), Some("Artist"));
        assert_eq!(document.metadata.title.as_deref(), Some("Song"));
        assert_eq!(document.metadata.offset_ms, 200);
        assert_eq!(
            document.timed_lines().unwrap(),
            [
                TimedLyricsLine {
                    id: 0,
                    start_ms: 12_200,
                    text: "One".to_owned(),
                },
                TimedLyricsLine {
                    id: 1,
                    start_ms: 15_600,
                    text: "Two".to_owned(),
                },
                TimedLyricsLine {
                    id: 2,
                    start_ms: 18_620,
                    text: "Chorus".to_owned(),
                },
                TimedLyricsLine {
                    id: 3,
                    start_ms: 60_620,
                    text: "Chorus".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn sorts_stably_clamps_negative_offset_and_queries_boundaries() {
        let document = parse_lyrics_text(
            "[offset:-1500]\n[00:02.000]Second\n[00:01.000]First A\n[00:01.000]First B\n",
            LyricsFormat::Lrc,
        )
        .unwrap();
        let lines = document.timed_lines().unwrap();
        assert_eq!(lines[0].text, "First A");
        assert_eq!(lines[0].start_ms, 0);
        assert_eq!(lines[1].text, "First B");
        assert_eq!(lines[2].text, "Second");
        assert_eq!(lines[2].start_ms, 500);
        assert_eq!(document.active_line_index(0), Some(1));
        assert_eq!(document.active_line_index(499), Some(1));
        assert_eq!(document.active_line_index(500), Some(2));
        assert!(document
            .diagnostics
            .iter()
            .any(|item| item.code == "negative_timestamp_clamped"));
    }

    #[test]
    fn returns_none_before_first_timed_line() {
        let document = parse_lyrics_text("[00:01.000]First", LyricsFormat::Lrc).unwrap();
        assert_eq!(document.active_line_index(999), None);
        assert_eq!(document.active_line(1_000).unwrap().text, "First");
    }

    #[test]
    fn display_uses_instrumental_token_outside_lyric_coverage() {
        let document = parse_lyrics_text(
            "[00:01.000]First\n[00:02.000]\n[00:03.000]Third",
            LyricsFormat::Lrc,
        )
        .unwrap();

        let before = document.display_at(999);
        assert!(before.is_instrumental());
        assert_eq!(before.display_text(), INSTRUMENTAL_LYRICS_TOKEN);

        let first = document.display_at(1_000);
        assert!(!first.is_instrumental());
        assert_eq!(first.display_text(), "First");

        let gap = document.display_at(2_500);
        assert!(gap.is_instrumental());
        assert_eq!(gap.display_text(), INSTRUMENTAL_LYRICS_TOKEN);

        let third = document.display_at(3_000);
        assert!(!third.is_instrumental());
        assert_eq!(third.display_text(), "Third");
    }

    #[test]
    fn plain_lyrics_have_no_timeline_coverage() {
        let document =
            parse_lyrics_text("First plain line\nSecond plain line", LyricsFormat::PlainText)
                .unwrap();

        let display = document.display_at(30_000);
        assert!(display.is_instrumental());
        assert_eq!(display.display_text(), INSTRUMENTAL_LYRICS_TOKEN);
        assert!(matches!(document.content, LyricsContent::Plain { .. }));
    }

    #[test]
    fn virtual_instrumental_document_uses_the_shared_token() {
        let document = LyricsDocument::instrumental();

        assert_eq!(document.format, LyricsFormat::Instrumental);
        assert_eq!(document.instrumental_token, INSTRUMENTAL_LYRICS_TOKEN);
        assert!(matches!(document.content, LyricsContent::Instrumental));
        assert!(document.display_at(0).is_instrumental());
        assert_eq!(
            document.display_at(u64::MAX).display_text(),
            INSTRUMENTAL_LYRICS_TOKEN
        );
    }

    #[test]
    fn txt_with_timestamps_is_detected_and_plain_lrc_degrades() {
        let timed =
            parse_lyrics_text("[ar:Artist][00:01.00]Timed", LyricsFormat::PlainText).unwrap();
        assert_eq!(timed.format, LyricsFormat::Lrc);
        assert!(matches!(timed.content, LyricsContent::Timed { .. }));

        let plain = parse_lyrics_text(
            "[ar:Artist]\nFirst plain line\nSecond plain line",
            LyricsFormat::Lrc,
        )
        .unwrap();
        assert!(matches!(
            plain.content,
            LyricsContent::Plain { ref text } if text == "First plain line\nSecond plain line"
        ));
    }

    #[test]
    fn rejects_invalid_timestamps_offsets_and_encodings() {
        assert!(parse_lyrics_text("[00:60.00]Bad", LyricsFormat::Lrc).is_err());
        assert!(parse_lyrics_text("[00:01.0000]Bad", LyricsFormat::Lrc).is_err());
        assert!(parse_lyrics_text("[offset:soon]\n[00:01]Bad", LyricsFormat::Lrc).is_err());

        let root = temp_dir("invalid_utf8");
        let path = root.join("bad.lrc");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(load_lyrics_file(&path).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn set_normalizes_extension_and_remove_cleans_all_sidecars() {
        let root = temp_dir("sidecars");
        let track = root.join("Song.ogg");
        fs::write(&track, []).unwrap();
        fs::write(root.join("Song.txt"), "old").unwrap();
        fs::write(root.join("Song.LYRICS"), "old duplicate").unwrap();
        let source = root.join("incoming.lyrics");
        fs::write(&source, "[00:01.00]New").unwrap();

        let asset = set_track_lyrics(&track, &source).unwrap();
        assert_eq!(asset.path, root.join("Song.lrc"));
        assert!(asset.path.is_file());
        assert!(!root.join("Song.txt").exists());
        assert!(!root.join("Song.LYRICS").exists());
        assert_eq!(load_track_lyrics(&track).unwrap().unwrap().path, asset.path);

        let removal = remove_track_lyrics(&track).unwrap();
        assert_eq!(removal.files_removed, 1);
        assert!(load_track_lyrics(&track).unwrap().is_none());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_set_preserves_existing_sidecar() {
        let root = temp_dir("preserve");
        let track = root.join("Song.ogg");
        let existing = root.join("Song.lrc");
        let invalid = root.join("invalid.lrc");
        fs::write(&track, []).unwrap();
        fs::write(&existing, "[00:01.00]Existing").unwrap();
        fs::write(&invalid, "[00:99.00]Invalid").unwrap();

        assert!(set_track_lyrics(&track, &invalid).is_err());
        assert_eq!(fs::read_to_string(existing).unwrap(), "[00:01.00]Existing");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn setting_an_existing_uppercase_lrc_keeps_the_normalized_sidecar_readable() {
        let root = temp_dir("uppercase");
        let track = root.join("Song.ogg");
        let source = root.join("Song.LRC");
        fs::write(&track, []).unwrap();
        fs::write(&source, "[00:01.00]Existing").unwrap();

        let asset = set_track_lyrics(&track, &source).unwrap();
        assert_eq!(asset.document.active_line(1_000).unwrap().text, "Existing");
        assert!(load_track_lyrics(&track).unwrap().is_some());
        fs::remove_dir_all(root).ok();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "silent_lyrics_{label}_{}_{}",
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
