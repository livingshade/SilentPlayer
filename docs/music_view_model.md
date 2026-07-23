# Music View Model

NormalPlayer treats every playable music entry as a **music view**. A view is a concrete representation of music that can be played by the player. A view only becomes an independent portable music identity when the user explicitly materializes or exports it.

The current imported file is not just a raw file record: it is the first view for that music item, also called the **primary view**. Future edits such as renaming, clipping time, lowering quality, changing cover art, or transcoding create derived views that point back to the same primary view. Derived views stay linked to their primary until the user chooses materialize/export.

## Identity Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `view_id` | Yes | No | Stable identity for this exact playable view. UI selection and track rows use this as the public id. Imported primary views use `audio:<audio_hash>`. Materialized primary views use an independent primary id that includes the audio hash and a materialization nonce. Derived views use a new stable id. |
| `primary_view_id` | Yes | No | Identity of the primary view this view was derived from. For primary views, this equals `view_id`. |
| `view_kind` | Yes | No | `primary` for originally imported managed audio, `derived` for transformed views. |
| `audio_hash` | Yes for primary views | No | Audio-content fingerprint used to deduplicate automatic imports and audit duplicate audio. Materialized views may intentionally share the same audio hash with their source while still receiving a new primary view id because the user explicitly forked them. |
| `file_hash` | Optional | No | Exact file bytes hash used for fast duplicate-file detection. It is stricter than `audio_hash` and can differ when metadata chunks differ. |

## Physical File Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `path` | Yes | Yes | Current concrete audio file for this view. Playback uses this path. Materialize/export writes a concrete file at the destination path. |
| `format_name` | Optional | No | Container/codec hint such as `mp3`, `flac`, `ogg`, or `wav`. Currently inferred from extension and used as metadata only. |
| `quality_profile` | Optional | No | Placeholder for future derived views such as `lossless`, `aac-256`, `preview-low`, or custom transcode presets. Missing values must not block playback. |
| `transform_spec` | Optional | No | Placeholder for the recipe that produced a derived view, for example trim ranges, target bitrate, artwork override, or normalization bake-in. Primary views normally have no transform spec because their edits are already materialized. |
| `size_bytes` / `modified_unix_seconds` | Optional | No | File fingerprint for detecting whether analysis cache is stale. |

## Display And User Fields

| Field | Required | Playback Required | Purpose |
| --- | --- | --- | --- |
| `title` / `artist` / `album` | Title required, others optional | No | Current display metadata. User edits update these fields. |
| `original_title` / `original_artist` / `original_album` | Original title required | No | Metadata captured at initial import. It is preserved even if display metadata changes. |
| `metadata_edited_at_unix_seconds` | Optional | No | Indicates user-edited display metadata. Metadata refresh should not overwrite user edits once this is set. |
| `artwork_count` | Yes | No | Count or effective signal of artwork known for this view. Embedded artwork increments this directly; managed track or album artwork assets may also set it so list rows can show an artwork affordance. |
| `track_notes` | Optional | No | User-written notes attached to the view. |
| `user_rating` | Optional | No | User rating for this view. `NULL` means unrated; valid stored values are 1 through 10. Ratings are display, sorting, and recommendation/history inputs, not playback requirements. |
| `artwork_assets` | Optional | No | Deduplicated managed image assets stored by content hash. User-selected track, album, and playlist covers are imported here once and then referenced by asset id; deleting or moving the original source image must not affect playback or display. |
| `track_artwork_refs` | Optional | No | Per-music cover override linking a view to an `artwork_assets.asset_id`. This is the highest-priority cover source. |
| `album_artwork_refs` | Optional | No | Per-album fallback cover links expanded onto the current tracks in that album. NormalPlayer does not have an album table, so changing an album cover enumerates matching music views and updates their fallback reference rows to the same `asset_id`. |
| `playlist_artwork_refs` | Optional | No | Playlist cover link to an `artwork_assets.asset_id`. If absent, playlist artwork falls back to the first track's resolved artwork. |
| `track_artwork` | Optional | No | Cached embedded artwork bytes extracted from imported audio files. This remains separate from user-selected managed artwork assets. |
| Sidecar lyrics | Optional | No | `.lrc`, `.txt`, or `.lyrics` file copied beside the managed audio view. Missing lyrics must not block playback. |

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
- The existing SQLite `tracks` table currently stores music views. The table name is historical; semantically, one row is one view.
- Export/materialize copies or renders the selected view to the destination path, registers that destination as a new primary view, sets `primary_view_id == view_id`, and keeps the source primary/derived views unchanged.
- Current materialization copies the audio bytes and sidecar files, and persists display metadata, rating, notes, artwork, and lyrics for the new primary view. Future tag-writing/transcoding will bake those changes into the audio container itself.
- Cover resolution is: per-music managed artwork asset, embedded/sidecar cover, then per-album managed artwork asset. If none exists, the UI shows no cover/placeholder.
- Missing optional fields such as `quality_profile`, `transform_spec`, artwork, lyrics, or loudness analysis are diagnostics only. They must not prevent playback.

## Extensibility Rules

1. Create a derived view when the app stores a selectable transformation that should remain linked to the source primary.
2. Create an independent primary view only for explicit materialize/export, and keep the source views unchanged.
3. Keep display-only edits on the view unless a product decision says display metadata should be shared across all views of the same primary.
4. Store future transform recipes in `transform_spec` as structured JSON once the transform set stabilizes.
5. Keep playback based on the view's concrete `path`; missing optional metadata should surface as UI diagnostics, not hard failures.
