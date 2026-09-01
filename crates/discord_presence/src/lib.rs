use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;

const DISCORD_PROTOCOL_VERSION: u32 = 1;
const OPCODE_HANDSHAKE: u32 = 0;
const OPCODE_FRAME: u32 = 1;
const OPCODE_CLOSE: u32 = 2;
const ACTIVITY_TYPE_LISTENING: u8 = 2;
const STATUS_DISPLAY_DETAILS: u8 = 2;
const MAX_ACTIVITY_TEXT_CHARACTERS: usize = 128;
const IPC_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceTrack<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_ms: Option<u64>,
    pub artwork_public_url: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ListeningActivity {
    #[serde(rename = "type")]
    activity_type: u8,
    details: String,
    state: String,
    status_display_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<ActivityAssets>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ActivityTimestamps {
    start: u64,
    end: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ActivityAssets {
    large_image: String,
    large_text: String,
}

impl ListeningActivity {
    pub fn from_track(track: PresenceTrack<'_>, position_ms: u64, now_unix_ms: u64) -> Self {
        let bounded_position = track
            .duration_ms
            .map_or(position_ms, |duration| position_ms.min(duration));
        let started_at = now_unix_ms.saturating_sub(bounded_position);
        let timestamps = track
            .duration_ms
            .filter(|duration| *duration > 0)
            .map(|duration| ActivityTimestamps {
                start: started_at,
                end: started_at.saturating_add(duration),
            });

        Self {
            activity_type: ACTIVITY_TYPE_LISTENING,
            details: activity_text(track.title, "Untitled"),
            state: activity_text(track.artist.unwrap_or_default(), "Unknown Artist"),
            status_display_type: STATUS_DISPLAY_DETAILS,
            timestamps,
            assets: meaningful_text(track.artwork_public_url).map(|url| ActivityAssets {
                large_image: url.to_owned(),
                large_text: activity_text(track.album.unwrap_or_default(), "Unknown Album"),
            }),
        }
    }
}

fn meaningful_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn activity_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    value.chars().take(MAX_ACTIVITY_TEXT_CHARACTERS).collect()
}

#[derive(Debug)]
pub struct DiscordPresence {
    application_id: String,
    connection: Option<IpcConnection>,
    last_activity: Option<ListeningActivity>,
}

impl DiscordPresence {
    pub fn new(application_id: impl Into<String>) -> Result<Self, DiscordPresenceError> {
        let application_id = application_id.into();
        if application_id.trim().is_empty() {
            return Err(DiscordPresenceError::Configuration(
                "Discord application ID is empty".to_owned(),
            ));
        }
        Ok(Self {
            application_id,
            connection: None,
            last_activity: None,
        })
    }

    pub fn update(&mut self, activity: ListeningActivity) -> Result<(), DiscordPresenceError> {
        if self.last_activity.as_ref() == Some(&activity) && self.connection.is_some() {
            return Ok(());
        }
        self.send_activity(Some(&activity))?;
        self.last_activity = Some(activity);
        Ok(())
    }

    pub fn connect(&mut self) -> Result<(), DiscordPresenceError> {
        self.ensure_connected()
    }

    pub fn clear(&mut self) -> Result<(), DiscordPresenceError> {
        if self.last_activity.is_none() {
            return Ok(());
        }
        self.send_activity(None)?;
        self.last_activity = None;
        Ok(())
    }

    fn send_activity(
        &mut self,
        activity: Option<&ListeningActivity>,
    ) -> Result<(), DiscordPresenceError> {
        self.ensure_connected()?;
        let payload = set_activity_payload(std::process::id(), activity);
        let result = self
            .connection
            .as_mut()
            .expect("connection was established")
            .send_frame(OPCODE_FRAME, &payload);
        if result.is_err() {
            self.connection = None;
        }
        result
    }

    fn ensure_connected(&mut self) -> Result<(), DiscordPresenceError> {
        if self.connection.is_some() {
            return Ok(());
        }
        self.connection = Some(IpcConnection::connect(&self.application_id)?);
        Ok(())
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        let _ = self.clear();
    }
}

#[derive(Debug)]
pub enum DiscordPresenceError {
    Configuration(String),
    Unavailable(String),
    Protocol(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for DiscordPresenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) | Self::Unavailable(message) | Self::Protocol(message) => {
                formatter.write_str(message)
            }
            Self::Io(error) => write!(formatter, "Discord IPC error: {error}"),
            Self::Json(error) => write!(formatter, "Discord JSON error: {error}"),
        }
    }
}

impl std::error::Error for DiscordPresenceError {}

impl From<io::Error> for DiscordPresenceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DiscordPresenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct IpcConnection {
    stream: std::os::unix::net::UnixStream,
}

#[cfg(unix)]
impl IpcConnection {
    fn connect(application_id: &str) -> Result<Self, DiscordPresenceError> {
        let socket_path = discord_socket_candidates()
            .into_iter()
            .find(|path| path.exists())
            .ok_or_else(|| {
                DiscordPresenceError::Unavailable(
                    "Discord desktop is not running or its IPC socket is unavailable".to_owned(),
                )
            })?;
        let stream = std::os::unix::net::UnixStream::connect(&socket_path).map_err(|error| {
            DiscordPresenceError::Unavailable(format!(
                "could not connect to Discord at {}: {error}",
                socket_path.display()
            ))
        })?;
        stream.set_read_timeout(Some(IPC_TIMEOUT))?;
        stream.set_write_timeout(Some(IPC_TIMEOUT))?;
        let mut connection = Self { stream };
        let handshake = json!({
            "v": DISCORD_PROTOCOL_VERSION,
            "client_id": application_id,
        });
        connection.send_frame(OPCODE_HANDSHAKE, &handshake)?;
        Ok(connection)
    }

    fn send_frame(&mut self, opcode: u32, value: &Value) -> Result<(), DiscordPresenceError> {
        let encoded = encode_frame(opcode, value)?;
        self.stream.write_all(&encoded)?;
        self.read_response()
    }

    fn read_response(&mut self) -> Result<(), DiscordPresenceError> {
        let mut header = [0_u8; 8];
        self.stream.read_exact(&mut header)?;
        let opcode = u32::from_le_bytes(header[0..4].try_into().expect("four-byte opcode"));
        let length = u32::from_le_bytes(header[4..8].try_into().expect("four-byte length"));
        let mut payload = vec![0_u8; length as usize];
        self.stream.read_exact(&mut payload)?;
        let response: Value = serde_json::from_slice(&payload)?;
        if opcode == OPCODE_CLOSE {
            return Err(DiscordPresenceError::Protocol(discord_close_message(
                &response,
            )));
        }
        if opcode != OPCODE_FRAME {
            return Err(DiscordPresenceError::Protocol(format!(
                "Discord returned unexpected IPC opcode {opcode}"
            )));
        }
        if response.get("evt").and_then(Value::as_str) == Some("ERROR") {
            return Err(DiscordPresenceError::Protocol(discord_error_message(
                &response,
            )));
        }
        Ok(())
    }
}

#[cfg(not(unix))]
#[derive(Debug)]
struct IpcConnection;

#[cfg(not(unix))]
impl IpcConnection {
    fn connect(_application_id: &str) -> Result<Self, DiscordPresenceError> {
        Err(DiscordPresenceError::Unavailable(
            "Discord IPC is currently supported by Silent only on macOS".to_owned(),
        ))
    }

    fn send_frame(&mut self, _opcode: u32, _value: &Value) -> Result<(), DiscordPresenceError> {
        Err(DiscordPresenceError::Unavailable(
            "Discord IPC is currently supported by Silent only on macOS".to_owned(),
        ))
    }
}

fn set_activity_payload(pid: u32, activity: Option<&ListeningActivity>) -> Value {
    json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": pid,
            "activity": activity,
        },
        "nonce": format!("silent-{pid}"),
    })
}

fn encode_frame(opcode: u32, value: &Value) -> Result<Vec<u8>, DiscordPresenceError> {
    let payload = serde_json::to_vec(value)?;
    let length = u32::try_from(payload.len()).map_err(|_| {
        DiscordPresenceError::Protocol("Discord IPC payload is too large".to_owned())
    })?;
    let mut frame = Vec::with_capacity(payload.len() + 8);
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn discord_error_message(response: &Value) -> String {
    response
        .pointer("/data/message")
        .and_then(Value::as_str)
        .map_or_else(
            || "Discord rejected the Rich Presence update".to_owned(),
            |message| format!("Discord rejected the Rich Presence update: {message}"),
        )
}

fn discord_close_message(response: &Value) -> String {
    let message = response
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Discord closed the IPC connection");
    match response.get("code").and_then(Value::as_u64) {
        Some(code) => format!("Discord closed the IPC connection ({code}): {message}"),
        None => message.to_owned(),
    }
}

fn discord_socket_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    push_env_root(&mut roots, "XDG_RUNTIME_DIR");
    push_env_root(&mut roots, "TMPDIR");
    roots.push(PathBuf::from("/tmp"));
    roots.sort();
    roots.dedup();

    roots
        .into_iter()
        .flat_map(|root| (0..10).map(move |index| root.join(format!("discord-ipc-{index}"))))
        .collect()
}

fn push_env_root(roots: &mut Vec<PathBuf>, variable: &str) {
    if let Some(path) = env::var_os(variable).filter(|value| !value.is_empty()) {
        roots.push(PathBuf::from(path));
    }
}

pub fn discord_desktop_available() -> bool {
    discord_socket_candidates().iter().any(|path| {
        fs::metadata(path)
            .map(|metadata| metadata.file_type().is_socket())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_activity_contains_track_and_progress() {
        let activity = ListeningActivity::from_track(
            PresenceTrack {
                title: "Song",
                artist: Some("Artist"),
                album: Some("Album"),
                duration_ms: Some(180_000),
                artwork_public_url: Some("https://example.com/song.jpg"),
            },
            30_000,
            1_000_000,
        );
        let value = serde_json::to_value(activity).unwrap();
        assert_eq!(value["type"], 2);
        assert_eq!(value["details"], "Song");
        assert_eq!(value["state"], "Artist");
        assert_eq!(value["status_display_type"], 2);
        assert_eq!(value["timestamps"]["start"], 970_000);
        assert_eq!(value["timestamps"]["end"], 1_150_000);
        assert_eq!(
            value["assets"]["large_image"],
            "https://example.com/song.jpg"
        );
        assert_eq!(value["assets"]["large_text"], "Album");
    }

    #[test]
    fn listening_activity_falls_back_and_bounds_text() {
        let long_album = "音".repeat(140);
        let activity = ListeningActivity::from_track(
            PresenceTrack {
                title: "Song",
                artist: Some(" "),
                album: Some(&long_album),
                duration_ms: None,
                artwork_public_url: Some("https://example.com/song.jpg"),
            },
            0,
            1,
        );
        let value = serde_json::to_value(activity).unwrap();
        assert_eq!(value["details"], "Song");
        assert_eq!(value["state"], "Unknown Artist");
        assert_eq!(
            value["assets"]["large_text"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            128
        );
        assert!(value.get("timestamps").is_none());
    }

    #[test]
    fn listening_activity_falls_back_when_album_and_artist_are_missing() {
        let activity = ListeningActivity::from_track(
            PresenceTrack {
                title: "Song",
                artist: None,
                album: None,
                duration_ms: None,
                artwork_public_url: None,
            },
            0,
            1,
        );
        let value = serde_json::to_value(activity).unwrap();
        assert_eq!(value["details"], "Song");
        assert_eq!(value["state"], "Unknown Artist");
    }

    #[test]
    fn frame_uses_little_endian_header_and_json_body() {
        let frame = encode_frame(OPCODE_FRAME, &json!({"hello": "discord"})).unwrap();
        assert_eq!(&frame[0..4], &OPCODE_FRAME.to_le_bytes());
        assert_eq!(
            u32::from_le_bytes(frame[4..8].try_into().unwrap()) as usize,
            frame.len() - 8
        );
        let value: Value = serde_json::from_slice(&frame[8..]).unwrap();
        assert_eq!(value["hello"], "discord");
    }

    #[test]
    fn clear_payload_sends_null_activity() {
        let value = set_activity_payload(42, None);
        assert_eq!(value["cmd"], "SET_ACTIVITY");
        assert_eq!(value["args"]["pid"], 42);
        assert!(value["args"]["activity"].is_null());
    }

    #[test]
    fn close_frame_reports_discord_message() {
        let response = json!({"code": 4000, "message": "Invalid Client ID"});
        assert_eq!(
            discord_close_message(&response),
            "Discord closed the IPC connection (4000): Invalid Client ID"
        );
    }

    #[test]
    fn current_machine_detects_running_discord_socket() {
        if discord_socket_candidates().iter().any(|path| path.exists()) {
            assert!(discord_desktop_available());
        }
    }
}
