use std::collections::{BTreeMap, BTreeSet};

/// Every shared product operation exported to the Apple targets must have an explicit CLI
/// decision. Platform/bootstrap plumbing is listed separately.
#[test]
fn every_shared_app_operation_has_a_cli_contract() {
    let source = include_str!("../../player_ffi/src/lib.rs");
    let exported = source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let remainder = line.strip_prefix("pub unsafe extern \"C\" fn player_app_")?;
            remainder
                .split(['(', ' '])
                .next()
                .map(|name| format!("player_app_{name}"))
        })
        .collect::<BTreeSet<_>>();

    let plumbing = ["player_app_create", "player_app_destroy"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let contracts = cli_contracts();
    let covered = contracts.keys().copied().collect::<BTreeSet<_>>();
    let expected = plumbing
        .iter()
        .copied()
        .chain(covered.iter().copied())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        exported, expected,
        "Update the Silent CLI contract whenever the shared App API changes"
    );
    assert!(
        contracts
            .values()
            .all(|command| command.starts_with("silent --cli ")),
        "all shared product commands must cross the explicit --cli boundary"
    );
}

fn cli_contracts() -> BTreeMap<&'static str, &'static str> {
    [
        (
            "player_app_export_library",
            "silent --cli library package export",
        ),
        (
            "player_app_import_library",
            "silent --cli library package import",
        ),
        ("player_app_zero_out_library", "silent --cli library zero"),
        (
            "player_app_import_folder",
            "silent --cli library import <folder>",
        ),
        (
            "player_app_import_files",
            "silent --cli library import <files>",
        ),
        ("player_app_library", "silent --cli library list"),
        ("player_app_library_page", "silent --cli library list"),
        ("player_app_search", "silent --cli library search"),
        ("player_app_analyze", "silent --cli library analyze"),
        ("player_app_audit_database", "silent --cli library audit"),
        ("player_app_user_data", "silent --cli user show"),
        (
            "player_app_play_library",
            "silent --cli playback shell: play-all",
        ),
        ("player_app_play_path", "silent --cli playback shell: play"),
        ("player_app_play_queue", "silent --cli playback shell: load"),
        ("player_app_pause", "silent --cli playback shell: pause"),
        ("player_app_resume", "silent --cli playback shell: resume"),
        (
            "player_app_audio_interruption_began",
            "silent --cli playback shell: lifecycle interruption-begin",
        ),
        (
            "player_app_audio_interruption_ended",
            "silent --cli playback shell: lifecycle interruption-end",
        ),
        (
            "player_app_audio_output_disconnected",
            "silent --cli playback shell: lifecycle output-disconnected",
        ),
        ("player_app_stop", "silent --cli playback shell: stop"),
        ("player_app_next", "silent --cli playback shell: next"),
        (
            "player_app_previous",
            "silent --cli playback shell: previous",
        ),
        ("player_app_seek", "silent --cli playback shell: seek"),
        ("player_app_poll", "silent --cli playback shell: status"),
        (
            "player_app_set_repeat_mode",
            "silent --cli playback shell: repeat",
        ),
        (
            "player_app_set_shuffle",
            "silent --cli playback shell: shuffle",
        ),
        ("player_app_queue", "silent --cli playback shell: queue"),
        ("player_app_track_details", "silent --cli track show"),
        ("player_app_edit_track_view", "silent --cli track edit"),
        ("player_app_set_track_notes", "silent --cli track notes set"),
        ("player_app_set_track_rating", "silent --cli track rate"),
        (
            "player_app_set_track_metadata",
            "silent --cli track metadata set",
        ),
        (
            "player_app_set_track_artwork",
            "silent --cli track artwork set",
        ),
        (
            "player_app_set_album_artwork",
            "silent --cli track album-artwork set",
        ),
        (
            "player_app_set_track_lyrics",
            "silent --cli track lyrics set",
        ),
        ("player_app_export_track_view", "silent --cli track export"),
        (
            "player_app_set_favorite",
            "silent --cli favorites add/remove",
        ),
        ("player_app_favorites", "silent --cli favorites list"),
        ("player_app_history", "silent --cli history list"),
        ("player_app_playlists", "silent --cli playlist list"),
        ("player_app_create_playlist", "silent --cli playlist create"),
        ("player_app_rename_playlist", "silent --cli playlist rename"),
        (
            "player_app_set_playlist_artwork",
            "silent --cli playlist artwork set",
        ),
        ("player_app_delete_playlist", "silent --cli playlist delete"),
        ("player_app_clear_playlist", "silent --cli playlist clear"),
        ("player_app_add_to_playlist", "silent --cli playlist add"),
        (
            "player_app_remove_from_playlist",
            "silent --cli playlist remove",
        ),
        (
            "player_app_move_playlist_track",
            "silent --cli playlist move",
        ),
        ("player_app_sort_playlist", "silent --cli playlist sort"),
        ("player_app_playlist_tracks", "silent --cli playlist show"),
    ]
    .into_iter()
    .collect()
}
