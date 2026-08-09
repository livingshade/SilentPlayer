use super::*;

#[test]
fn analyzed_rows_require_an_explicit_analysis_version() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut track = Track::from_path("/music/song.flac".into());
    track.loudness = Some(LoudnessInfo::track(-18.0, -2.0));
    store.upsert_track(&track).unwrap();
    store
        .conn
        .execute(
            "UPDATE tracks SET analysis_version = NULL WHERE path = ?1",
            params!["/music/song.flac"],
        )
        .unwrap();

    assert!(store.track_by_path("/music/song.flac").is_err());
}

#[test]
fn finds_pending_analysis_and_updates_cache() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut track = Track::from_path("/music/pending.ogg".into());
    track.fingerprint = Some(FileFingerprint::new(10, 1));
    store.upsert_track(&track).unwrap();

    assert_eq!(store.pending_analysis(1, None).unwrap().len(), 1);
    store
        .save_loudness(
            &track.path,
            track.fingerprint,
            LoudnessInfo::track(-12.0, -1.0),
        )
        .unwrap();

    assert_eq!(store.pending_analysis(1, None).unwrap().len(), 0);
    assert_eq!(store.pending_analysis(2, None).unwrap().len(), 1);
}

#[test]
fn fingerprint_change_marks_track_pending_again() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut analyzed = Track::from_path("/music/song.ogg".into());
    analyzed.fingerprint = Some(FileFingerprint {
        size_bytes: 1,
        modified_unix_seconds: 10,
    });
    analyzed.loudness = Some(LoudnessInfo::track(-12.0, -1.0));
    store.upsert_track(&analyzed).unwrap();
    assert_eq!(store.pending_analysis(1, None).unwrap().len(), 0);

    let mut changed = Track::from_path("/music/song.ogg".into());
    changed.fingerprint = Some(FileFingerprint {
        size_bytes: 2,
        modified_unix_seconds: 10,
    });
    store.upsert_track(&changed).unwrap();

    assert_eq!(store.pending_analysis(1, None).unwrap().len(), 1);
}

#[test]
fn groups_album_tracks_and_saves_album_loudness() {
    let mut store = LibraryStore::in_memory().unwrap();
    let first = analyzed_album_track("/music/02.ogg", "Album", "Band", 2, -20.0);
    let second = analyzed_album_track("/music/01.ogg", "Album", "Band", 1, -10.0);
    store.upsert_tracks(&[first, second]).unwrap();

    let groups = store.album_groups().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].album, "Album");
    assert_eq!(groups[0].album_artist.as_deref(), Some("Band"));
    assert_eq!(groups[0].tracks[0].track_number, Some(1));

    let paths = groups[0]
        .tracks
        .iter()
        .map(|track| track.path.clone())
        .collect::<Vec<_>>();
    let updated = store
        .save_album_loudness_for_paths(&paths, -12.0, -0.5, 9)
        .unwrap();
    assert_eq!(updated, 2);

    let loaded = store.track_by_path("/music/01.ogg").unwrap().unwrap();
    let loudness = loaded.loudness.unwrap();
    assert_eq!(loudness.integrated_lufs, -10.0);
    assert_eq!(loudness.album_integrated_lufs, Some(-12.0));
    assert_eq!(loudness.album_true_peak_dbtp, Some(-0.5));
    assert_eq!(loudness.analysis_version, 9);
}
