use super::*;

#[test]
fn saves_and_reads_artwork_cache() {
    let mut store = LibraryStore::in_memory().unwrap();
    let track = Track::from_path("/music/song.ogg".into());
    store.upsert_track(&track).unwrap();
    let images = vec![ArtworkImage {
        picture_index: 0,
        mime_type: Some("image/png".to_owned()),
        picture_type: "CoverFront".to_owned(),
        description: Some("front".to_owned()),
        data: vec![1, 2, 3, 4],
    }];

    let saved = store.save_artwork("/music/song.ogg", &images).unwrap();
    let loaded = store.artwork_for_path("/music/song.ogg").unwrap();
    let summaries = store.artwork_summaries().unwrap();
    let track = store.track_by_path("/music/song.ogg").unwrap().unwrap();

    assert_eq!(saved, 1);
    assert_eq!(loaded, images);
    assert_eq!(summaries[0].image_count, 1);
    assert_eq!(summaries[0].byte_count, 4);
    assert_eq!(track.artwork_count, 1);
}

#[test]
fn resolves_track_artwork_before_album_artwork_with_deduped_assets() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut first = Track::from_path("/music/album/01.ogg".into());
    first.title = "First".to_owned();
    first.album = Some("Shared Album".to_owned());
    first.album_artist = Some("Band".to_owned());
    let mut second = Track::from_path("/music/album/02.ogg".into());
    second.title = "Second".to_owned();
    second.album = Some("Shared Album".to_owned());
    second.artist = Some("Band".to_owned());
    let mut other_artist = Track::from_path("/music/other/01.ogg".into());
    other_artist.title = "Other".to_owned();
    other_artist.album = Some("Shared Album".to_owned());
    other_artist.artist = Some("Other Band".to_owned());
    store
        .upsert_tracks(&[first.clone(), second.clone(), other_artist.clone()])
        .unwrap();
    let album_artwork = artwork_image(0, vec![1, 2, 3]);
    let track_artwork = artwork_image(0, vec![4, 5, 6]);

    assert_eq!(
        store
            .set_album_artwork_reference_for_track(&first.path, &album_artwork)
            .unwrap(),
        2
    );
    let first_reference = store
        .effective_artwork_reference(&first.path)
        .unwrap()
        .unwrap();
    assert_eq!(first_reference.scope, ArtworkReferenceScope::Album);
    assert_eq!(first_reference.image.data, album_artwork.data);
    assert_eq!(
        store
            .effective_artwork_reference(&second.path)
            .unwrap()
            .unwrap()
            .scope,
        ArtworkReferenceScope::Album
    );
    assert_eq!(artwork_asset_count(&store), 1);
    assert!(store
        .effective_artwork_reference(&other_artist.path)
        .unwrap()
        .is_none());
    assert!(store.artwork_for_path(&first.path).unwrap().is_empty());

    assert_eq!(
        store
            .set_track_artwork_reference(&second.path, &track_artwork)
            .unwrap(),
        1
    );
    let second_reference = store
        .effective_artwork_reference(&second.path)
        .unwrap()
        .unwrap();
    assert_eq!(second_reference.scope, ArtworkReferenceScope::Track);
    assert_eq!(second_reference.image.data, track_artwork.data);
    assert_eq!(artwork_asset_count(&store), 2);

    let materialized = Track::from_path("/music/materialized/second.ogg".into());
    store.upsert_track(&materialized).unwrap();
    store
        .copy_artwork_references(&second.path, &materialized.path)
        .unwrap();
    assert_eq!(
        store
            .track_artwork_reference(&materialized.path)
            .unwrap()
            .unwrap()
            .image
            .data,
        vec![4, 5, 6]
    );
    assert_eq!(
        store
            .album_artwork_reference(&materialized.path)
            .unwrap()
            .unwrap()
            .image
            .data,
        vec![1, 2, 3]
    );
    assert_eq!(artwork_asset_count(&store), 2);
}

#[test]
fn artwork_save_replaces_previous_images_and_can_clear_cache() {
    let mut store = LibraryStore::in_memory().unwrap();
    let track = Track::from_path("/music/song.ogg".into());
    store.upsert_track(&track).unwrap();
    store
        .save_artwork(
            "/music/song.ogg",
            &[
                artwork_image(0, vec![1, 2, 3]),
                artwork_image(1, vec![4, 5]),
            ],
        )
        .unwrap();

    store
        .save_artwork("/music/song.ogg", &[artwork_image(0, vec![9])])
        .unwrap();
    let loaded = store.artwork_for_path("/music/song.ogg").unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].data, vec![9]);
    assert_eq!(
        store
            .track_by_path("/music/song.ogg")
            .unwrap()
            .unwrap()
            .artwork_count,
        1
    );

    store.save_artwork("/music/song.ogg", &[]).unwrap();
    assert!(store
        .artwork_for_path("/music/song.ogg")
        .unwrap()
        .is_empty());
    assert!(store.artwork_summaries().unwrap().is_empty());
    assert_eq!(
        store
            .track_by_path("/music/song.ogg")
            .unwrap()
            .unwrap()
            .artwork_count,
        0
    );
}

#[test]
fn public_artwork_urls_are_stored_per_track_and_replaced_atomically() {
    let mut store = LibraryStore::in_memory().unwrap();
    let first = Track::from_path("/music/album/01.ogg".into());
    let second = Track::from_path("/music/album/02.ogg".into());
    store
        .upsert_tracks(&[first.clone(), second.clone()])
        .unwrap();

    let shared_url = "https://livingshade.github.io/silent/cover-a.jpg".to_owned();
    assert_eq!(
        store
            .replace_track_artwork_public_urls(&[
                (first.path.clone(), shared_url.clone()),
                (second.path.clone(), shared_url.clone()),
            ])
            .unwrap(),
        2
    );
    assert_eq!(
        store.track_artwork_public_url(&first.path).unwrap(),
        Some(shared_url.clone())
    );
    assert_eq!(
        store.track_artwork_public_url(&second.path).unwrap(),
        Some(shared_url)
    );

    let replacement = "https://livingshade.github.io/silent/cover-b.png".to_owned();
    store
        .replace_track_artwork_public_urls(&[(first.path.clone(), replacement.clone())])
        .unwrap();
    assert_eq!(
        store.track_artwork_public_url(&first.path).unwrap(),
        Some(replacement)
    );
    assert_eq!(store.track_artwork_public_url(&second.path).unwrap(), None);
}
