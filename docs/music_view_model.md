# Music View Model

NormalPlayer stores every independent playable song as exactly one **primary music view**. The view is both the song's stable public identity and its current playable representation. Library rows, playlist items, playback queues, metadata, artwork, lyrics, notes, ratings, and analysis all refer to that same identity.

Editing a song updates its primary view in place and preserves the same song identity throughout Library, playlists, and playback.

## Identity Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `view_id` | Yes | No | Stable public identity for the song. Imported songs normally use `audio:<audio_hash>`. An explicitly materialized/exported copy receives a new independent id. |
| `primary_view_id` | Yes | No | The song's primary identity. It must equal `view_id`. |
| `view_kind` | Yes | No | Must be `primary`. |
| `audio_hash` | Yes | No | Audio-content fingerprint used to deduplicate imports and audit duplicate audio. Explicit materialization may intentionally create an independent song with the same audio content and a different `view_id`. |
| `file_hash` | Optional | No | Exact file bytes hash used for fast duplicate-file detection. It is stricter than `audio_hash` and can differ when metadata chunks differ. |

## Physical File Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `path` | Yes | Yes | Current concrete audio file for the song. Playback uses this path. Materialize/export writes a separate concrete file at the destination path. |
| `format_name` | Optional | No | Container/codec hint such as `mp3`, `flac`, `ogg`, or `wav`. Currently inferred from extension and used as metadata only. |
| `size_bytes` / `modified_unix_seconds` | Optional | No | File fingerprint for detecting whether analysis cache is stale. |

## Display And User Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `title` / `artist` / `album` | Title required, others optional | No | Current display metadata. User edits update the primary song in place. |
| `original_title` / `original_artist` / `original_album` | Original title required | No | Metadata captured at initial import. It is preserved even if display metadata changes. |
| `metadata_edited_at_unix_seconds` | Optional | No | Indicates user-edited display metadata. Metadata refresh should not overwrite user edits once this is set. |
| `artwork_count` | Yes | No | Count or effective signal of artwork known for this song. Embedded artwork increments this directly; managed track or album artwork assets may also set it so list rows can show an artwork affordance. |
| `track_notes` | Optional | No | User-written notes attached to the song. |
| `user_rating` | Optional | No | User rating for this song. `NULL` means unrated; valid stored values are 1 through 10. Ratings are display, sorting, and recommendation/history inputs, not playback requirements. |
| `artwork_assets` | Optional | No | Deduplicated managed image assets stored by content hash. User-selected track, album, and playlist covers are imported here once and then referenced by asset id; deleting or moving the original source image must not affect playback or display. |
| `track_artwork_refs` | Optional | No | Per-song cover override linking the song to an `artwork_assets.asset_id`. This is the highest-priority cover source. |
| `album_artwork_refs` | Optional | No | Per-album fallback cover links expanded onto the current songs in that album. NormalPlayer does not have an album table, so changing an album cover enumerates matching songs and updates their fallback reference rows to the same `asset_id`. |
| `playlist_artwork_refs` | Optional | No | Playlist cover link to an `artwork_assets.asset_id`. If absent, playlist artwork falls back to the first track's resolved artwork. |
| `track_artwork` | Optional | No | Cached embedded artwork bytes extracted from imported audio files. This remains separate from user-selected managed artwork assets. |
| Sidecar lyrics | Optional | No | `.lrc`, `.txt`, or `.lyrics` file stored beside the managed audio file. Lyrics edits update this primary song's sidecar in place. Missing lyrics must not block playback. |

## Loudness Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `integrated_lufs` / `true_peak_dbtp` | Optional | No | Track loudness analysis. Missing values mean playback falls back to unity gain or pending normalize status. |
| `album_integrated_lufs` / `album_true_peak_dbtp` | Optional | No | Album-mode loudness cache. |
| `analysis_version` | Optional | No | Version marker for invalidating old analysis. |
| `analysis_size_bytes` / `analysis_modified_unix_seconds` | Optional | No | File fingerprint captured when loudness was analyzed. |

## Current Behavior

- Import copies source audio into the managed media directory and creates a primary view.
- Primary view id is `audio:<audio_hash>`.
- Import deduplication uses `audio_hash`, so different filenames or metadata tags do not create duplicate primary views for identical audio.
- Every row in the SQLite `tracks` table is an independent primary song with `primary_view_id == view_id` and `view_kind == primary`.
- Metadata, artwork, lyrics, notes, ratings, format, and audio-content edits update that song in place. They do not create another view.
- Library, playlists, and playback queues all use the same primary song identity.
- Export/materialize copies or renders the song to the destination path and registers that destination as a new independent primary song. The source song remains unchanged.
- Current materialization copies the audio bytes and sidecar files, and persists display metadata, rating, notes, artwork, and lyrics for the new song. Future tag-writing/transcoding will bake those changes into the new audio container itself.
- Cover resolution is: per-music managed artwork asset, embedded/sidecar cover, then per-album managed artwork asset. If none exists, the UI shows no cover/placeholder.
- Missing optional fields such as artwork, lyrics, or loudness analysis are diagnostics only. They must not prevent playback.

## Extensibility Rules

1. Keep one primary row and one public identity per song.
2. Apply metadata, artwork, lyrics, format, and audio-content changes to that song in place.
3. If the user explicitly wants to preserve both results as separate songs, materialize/export first and give the result its own primary identity.
4. Make Library, playlist, search, history, and playback APIs consume the same song identity without client-side fallback selection.
5. Keep playback based on the song's concrete `path`; missing optional metadata should surface as UI diagnostics, not hard failures.
