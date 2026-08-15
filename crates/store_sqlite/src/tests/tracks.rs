use super::*;

#[test]
fn stores_and_loads_track_metadata_and_loudness() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut track = Track::from_path("/music/song.flac".into());
    track.title = "Song".to_owned();
    track.artist = Some("Artist".to_owned());
    track.album = Some("Album".to_owned());
    track.album_artist = Some("Album Artist".to_owned());
    track.genre = Some("Rock".to_owned());
    track.track_number = Some(3);
    track.disc_number = Some(1);
    track.year = Some(2026);
    track.duration_ms = Some(12_345);
    track.artwork_count = 2;
    track.fingerprint = Some(FileFingerprint::new(99, 1));
    track.file_hash = Some("file-hash".to_owned());
    track.view_name = Some("Reference view".to_owned());
    track.user_rating = Some(9);
    track.set_primary_audio_hash("audio-hash");
    track.loudness = Some(LoudnessInfo {
        integrated_lufs: -18.0,
        true_peak_dbtp: -2.0,
        album_integrated_lufs: Some(-17.0),
        album_true_peak_dbtp: Some(-1.5),
        analysis_version: 7,
    });

    store.upsert_track(&track).unwrap();
    let loaded = store.track_by_path("/music/song.flac").unwrap().unwrap();

    assert_eq!(loaded.title, "Song");
    assert_eq!(loaded.artist.as_deref(), Some("Artist"));
    assert_eq!(loaded.album_artist.as_deref(), Some("Album Artist"));
    assert_eq!(loaded.track_number, Some(3));
    assert_eq!(loaded.duration_ms, Some(12_345));
    assert_eq!(loaded.artwork_count, 2);
    assert_eq!(loaded.file_hash.as_deref(), Some("file-hash"));
    assert_eq!(loaded.audio_hash.as_deref(), Some("audio-hash"));
    assert_eq!(loaded.view_id.value(), "audio:audio-hash");
    assert_eq!(loaded.primary_view_id.value(), "audio:audio-hash");
    assert_eq!(loaded.view_kind, TrackViewKind::Primary);
    assert_eq!(loaded.format_name.as_deref(), Some("flac"));
    assert_eq!(loaded.view_name.as_deref(), Some("Reference view"));
    assert_eq!(loaded.user_rating, Some(9));
    assert!(loaded.transform_spec.is_none());
    assert!(loaded.quality_profile.is_none());
    assert_eq!(loaded.loudness.unwrap().analysis_version, 7);
    let metadata = store.track_metadata(&track.path).unwrap().unwrap();
    assert_eq!(metadata.view_id, "audio:audio-hash");
    assert_eq!(metadata.primary_view_id, "audio:audio-hash");
    assert_eq!(metadata.view_kind, "primary");
    assert_eq!(metadata.format_name.as_deref(), Some("flac"));
    assert_eq!(metadata.view_name.as_deref(), Some("Reference view"));
    assert_eq!(metadata.user_rating, Some(9));
    assert_eq!(
        store.track_by_file_hash("file-hash").unwrap().unwrap().path,
        track.path
    );
    assert_eq!(
        store
            .track_by_audio_hash("audio-hash")
            .unwrap()
            .unwrap()
            .path,
        track.path
    );
}

#[test]
fn replaces_track_paths_and_zeroes_out_everything() {
    let mut store = LibraryStore::in_memory().unwrap();
    let old_path = PathBuf::from("/old-library/song.ogg");
    let new_path = PathBuf::from("/new-library/song.ogg");
    let mut track = Track::from_path(old_path.clone());
    track.title = "Portable Song".to_owned();
    track.artist = Some("Portable Artist".to_owned());
    track.album = Some("Portable Album".to_owned());
    track.set_primary_audio_hash("portable-audio");
    store.upsert_track(&track).unwrap();

    let image = ArtworkImage {
        picture_index: 0,
        mime_type: Some("image/png".to_owned()),
        picture_type: "CoverFront".to_owned(),
        description: Some("portable".to_owned()),
        data: vec![4, 5, 6],
    };
    store.create_playlist("Portable List").unwrap();
    store
        .add_playlist_track("Portable List", &old_path)
        .unwrap();
    store.set_favorite(&old_path, true).unwrap();
    store.record_playback(&old_path, 321, true).unwrap();
    store.set_track_notes(&old_path, "portable note").unwrap();
    store
        .save_artwork(&old_path, std::slice::from_ref(&image))
        .unwrap();
    store
        .set_track_artwork_reference(&old_path, &image)
        .unwrap();
    store
        .set_album_artwork_reference_for_track(&old_path, &image)
        .unwrap();
    store
        .save_playlist_artwork("Portable List", &image)
        .unwrap();

    store
        .replace_track_paths(&[(old_path.clone(), new_path.clone())])
        .unwrap();

    assert!(store.track_by_path(&old_path).unwrap().is_none());
    assert_eq!(
        store.track_by_path(&new_path).unwrap().unwrap().title,
        "Portable Song"
    );
    assert_eq!(
        store.playlist_tracks("Portable List").unwrap()[0]
            .track
            .path,
        new_path
    );
    assert_eq!(store.favorite_tracks().unwrap()[0].path, new_path);
    assert_eq!(store.play_history(10).unwrap()[0].track.path, new_path);
    assert_eq!(
        store.track_notes(&new_path).unwrap().as_deref(),
        Some("portable note")
    );
    assert_eq!(
        store.artwork_for_path(&new_path).unwrap()[0].data,
        image.data
    );
    assert!(store.track_artwork_reference(&new_path).unwrap().is_some());
    assert!(store.album_artwork_reference(&new_path).unwrap().is_some());
    assert!(store.playlist_artwork("Portable List").unwrap().is_some());

    store.zero_out().unwrap();

    assert!(store.tracks().unwrap().is_empty());
    assert!(store.playlists().unwrap().is_empty());
    assert!(store.favorite_tracks().unwrap().is_empty());
    assert!(store.play_history(10).unwrap().is_empty());
    assert!(store.artwork_summaries().unwrap().is_empty());
}

#[test]
fn upsert_preserves_existing_loudness_when_metadata_refresh_has_none() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut analyzed = Track::from_path("/music/song.ogg".into());
    analyzed.fingerprint = Some(FileFingerprint {
        size_bytes: 1,
        modified_unix_seconds: 10,
    });
    analyzed.loudness = Some(LoudnessInfo::track(-12.0, -1.0));
    store.upsert_track(&analyzed).unwrap();

    let mut metadata_refresh = Track::from_path("/music/song.ogg".into());
    metadata_refresh.title = "Fresh".to_owned();
    metadata_refresh.fingerprint = analyzed.fingerprint;
    store.set_track_rating(&analyzed.path, Some(8)).unwrap();
    store.upsert_track(&metadata_refresh).unwrap();

    let loaded = store.track_by_path("/music/song.ogg").unwrap().unwrap();
    assert_eq!(loaded.title, "Fresh");
    assert!(loaded.loudness.is_some());
    assert_eq!(loaded.user_rating, Some(8));
}

#[test]
fn sets_clears_and_validates_track_rating() {
    let mut store = LibraryStore::in_memory().unwrap();
    let track = Track::from_path("/music/rated.ogg".into());
    store.upsert_track(&track).unwrap();

    assert_eq!(store.set_track_rating(&track.path, Some(10)).unwrap(), 1);
    assert_eq!(
        store
            .track_by_path(&track.path)
            .unwrap()
            .unwrap()
            .user_rating,
        Some(10)
    );
    assert_eq!(
        store
            .track_metadata(&track.path)
            .unwrap()
            .unwrap()
            .user_rating,
        Some(10)
    );
    assert!(store.set_track_rating(&track.path, Some(0)).is_err());
    assert!(store.set_track_rating(&track.path, Some(11)).is_err());

    assert_eq!(store.set_track_rating(&track.path, None).unwrap(), 1);
    assert_eq!(
        store
            .track_by_path(&track.path)
            .unwrap()
            .unwrap()
            .user_rating,
        None
    );
}

#[test]
fn searches_across_core_track_fields() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut ocean = Track::from_path("/music/ocean.ogg".into());
    ocean.title = "Ocean Chorus".to_owned();
    ocean.artist = Some("Sea Band".to_owned());
    ocean.album = Some("Blue Album".to_owned());
    let mut mountain = Track::from_path("/music/mountain.ogg".into());
    mountain.title = "Mountain Theme".to_owned();
    store.upsert_tracks(&[ocean, mountain]).unwrap();

    let results = store.search_tracks("sea", 10).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Ocean Chorus");
}

#[test]
fn rejects_zero_track_search_limit() {
    let store = LibraryStore::in_memory().unwrap();

    let error = store.search_tracks("ocean", 0).unwrap_err();

    assert!(error.to_string().contains("greater than zero"));
}

#[test]
fn search_treats_like_metacharacters_as_literals() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut literal = Track::from_path("/music/literal.ogg".into());
    literal.title = "100%_Literal".to_owned();
    let mut wildcard_decoy = Track::from_path("/music/decoy.ogg".into());
    wildcard_decoy.title = "100xxLiteral".to_owned();
    store.upsert_tracks(&[literal, wildcard_decoy]).unwrap();

    let percent_results = store.search_tracks("100%", 10).unwrap();
    let underscore_results = store.search_tracks("%_", 10).unwrap();

    assert_eq!(percent_results.len(), 1);
    assert_eq!(percent_results[0].title, "100%_Literal");
    assert_eq!(underscore_results.len(), 1);
    assert_eq!(underscore_results[0].title, "100%_Literal");
}

#[test]
fn stores_track_notes_and_merges_duplicate_track_references() {
    let mut store = LibraryStore::in_memory().unwrap();
    let canonical = Track::from_path("/music/a.ogg".into());
    let duplicate = Track::from_path("/music/b.ogg".into());
    store
        .upsert_tracks(&[canonical.clone(), duplicate.clone()])
        .unwrap();
    store.create_playlist("Mix").unwrap();
    store.add_playlist_track("Mix", &canonical.path).unwrap();
    store.add_playlist_track("Mix", &duplicate.path).unwrap();
    store.set_favorite(&duplicate.path, true).unwrap();
    store.record_playback(&duplicate.path, 99, true).unwrap();
    store
        .set_track_notes(&canonical.path, "canonical note")
        .unwrap();
    store
        .set_track_notes(&duplicate.path, "duplicate note")
        .unwrap();
    store.set_track_rating(&duplicate.path, Some(7)).unwrap();
    store
        .set_track_artwork_reference(&duplicate.path, &artwork_image(0, vec![4, 5, 6]))
        .unwrap();
    store
        .save_artwork(&duplicate.path, &[artwork_image(0, vec![7, 8, 9])])
        .unwrap();

    assert!(store
        .merge_duplicate_track(&canonical.path, &duplicate.path)
        .unwrap());

    assert!(store.track_by_path(&duplicate.path).unwrap().is_none());
    let playlist_tracks = store.playlist_tracks("Mix").unwrap();
    assert_eq!(playlist_tracks.len(), 1);
    assert_eq!(playlist_tracks[0].position, 0);
    assert_eq!(playlist_tracks[0].track.path, canonical.path);
    assert_eq!(store.favorite_tracks().unwrap()[0].path, canonical.path);
    assert_eq!(
        store.play_history(10).unwrap()[0].track.path,
        canonical.path
    );
    assert!(store
        .track_notes(&canonical.path)
        .unwrap()
        .unwrap()
        .contains("duplicate note"));
    assert_eq!(
        store.artwork_for_path(&canonical.path).unwrap()[0].data,
        vec![7, 8, 9]
    );
    assert_eq!(
        store
            .track_by_path(&canonical.path)
            .unwrap()
            .unwrap()
            .user_rating,
        Some(7)
    );
    assert_eq!(
        store
            .track_artwork_reference(&canonical.path)
            .unwrap()
            .unwrap()
            .image
            .data,
        vec![4, 5, 6]
    );
}

#[test]
fn deterministic_random_user_workflow_preserves_store_invariants() {
    let mut store = LibraryStore::in_memory().unwrap();
    let tracks = (0..12)
        .map(|index| {
            let mut track = Track::from_path(format!("/music/random/{index:02}.ogg").into());
            track.title = format!("Song {index:02}");
            track.artist = Some(format!("Artist {}", index % 3));
            track.album = Some(format!("Album {}", index % 2));
            track.duration_ms = Some(30_000 + index as u64);
            track
        })
        .collect::<Vec<_>>();
    store.upsert_tracks(&tracks).unwrap();

    let paths = tracks
        .iter()
        .map(|track| track.path.clone())
        .collect::<Vec<_>>();
    let playlist_names = ["Mix", "Commute", "Late"];
    let mut rng = TestRng::new(0xC0FFEE);
    let mut expected_favorites = BTreeSet::new();
    let mut expected_playlist_counts = BTreeMap::<String, usize>::new();
    let mut expected_history_count = 0_usize;
    let mut expected_artwork = BTreeMap::<PathBuf, (usize, usize)>::new();
    let mut expected_ratings = BTreeMap::<PathBuf, Option<u8>>::new();

    for step in 0..250 {
        let path = paths[rng.usize(paths.len())].clone();
        match rng.usize(7) {
            0 => {
                let query = format!("artist {}", rng.usize(3));
                let results = store.search_tracks(&query, 50).unwrap();
                assert!(!results.is_empty(), "query={query}");
                assert!(results.iter().all(|track| {
                    track
                        .artist
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                }));
            }
            1 => {
                let name = playlist_names[rng.usize(playlist_names.len())].to_owned();
                let added = store.add_playlist_track(&name, &path).unwrap();
                if added {
                    *expected_playlist_counts.entry(name).or_default() += 1;
                }
            }
            2 => {
                let enabled = rng.bool();
                store.set_favorite(&path, enabled).unwrap();
                if enabled {
                    expected_favorites.insert(path);
                } else {
                    expected_favorites.remove(&path);
                }
            }
            3 => {
                let position_ms = rng.usize(240_000) as u64;
                let completed = rng.bool();
                store
                    .record_playback(&path, position_ms, completed)
                    .unwrap();
                expected_history_count += 1;
            }
            4 => {
                let image_count = rng.usize(3);
                let images = (0..image_count)
                    .map(|index| {
                        let byte_count = 1 + rng.usize(8);
                        artwork_image(index as u32, vec![index as u8; byte_count])
                    })
                    .collect::<Vec<_>>();
                let byte_count = images.iter().map(|image| image.data.len()).sum();
                store.save_artwork(&path, &images).unwrap();
                if image_count == 0 {
                    expected_artwork.remove(&path);
                } else {
                    expected_artwork.insert(path, (image_count, byte_count));
                }
            }
            5 => {
                let rating = if rng.bool() {
                    Some((1 + rng.usize(10)) as u8)
                } else {
                    None
                };
                store.set_track_rating(&path, rating).unwrap();
                expected_ratings.insert(path, rating);
            }
            _ => {
                let loaded = store.track_by_path(&path).unwrap().unwrap();
                assert_eq!(loaded.path, path);
                assert!(loaded.title.starts_with("Song "));
            }
        }

        assert_eq!(store.count_tracks().unwrap(), tracks.len(), "step={step}");
        assert_eq!(
            paths
                .iter()
                .map(|path| store.track_by_path(path).unwrap().is_some())
                .filter(|exists| *exists)
                .count(),
            tracks.len(),
            "step={step}"
        );
        assert_eq!(
            store.favorite_tracks().unwrap().len(),
            expected_favorites.len(),
            "step={step}"
        );

        let actual_playlist_counts = store
            .playlists()
            .unwrap()
            .into_iter()
            .map(|summary| (summary.name, summary.track_count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            actual_playlist_counts, expected_playlist_counts,
            "step={step}"
        );

        assert_eq!(
            store.play_history(1_000).unwrap().len(),
            expected_history_count,
            "step={step}"
        );

        let actual_artwork = store
            .artwork_summaries()
            .unwrap()
            .into_iter()
            .map(|summary| (summary.path, (summary.image_count, summary.byte_count)))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual_artwork, expected_artwork, "step={step}");

        for (path, expected_rating) in &expected_ratings {
            assert_eq!(
                store.track_by_path(path).unwrap().unwrap().user_rating,
                *expected_rating,
                "step={step}"
            );
        }
    }
}
