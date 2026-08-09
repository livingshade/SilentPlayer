use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use errors::PlayerResult;
use fingerprint::{audio_hash, file_hash};
use library_fs::fingerprint_from_metadata;

use crate::dto::AuditSummary;
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn audit_database(&self) -> PlayerResult<AuditSummary> {
        let mut store = self.store()?;
        let mut tracks = store.tracks()?;
        let tracks_scanned = tracks.len();
        let mut hashes_updated = 0_usize;
        let mut failures = 0_usize;

        for track in &mut tracks {
            let mut changed = false;
            if track.file_hash.is_none() {
                match file_hash(&track.path) {
                    Ok(hash) => {
                        track.file_hash = Some(hash);
                        changed = true;
                    }
                    Err(_) => failures += 1,
                }
            }
            match audio_hash(&track.path) {
                Ok(fingerprint) => {
                    if track.audio_hash.as_deref() != Some(fingerprint.hash.as_str()) {
                        track.set_primary_audio_hash(fingerprint.hash);
                        changed = true;
                    }
                }
                Err(_) => failures += 1,
            }
            if changed {
                let fingerprint = match fs::metadata(&track.path) {
                    Ok(metadata) => Some(fingerprint_from_metadata(&metadata)),
                    Err(_) => {
                        failures += 1;
                        None
                    }
                };
                store.update_track_hashes(
                    &track.path,
                    track.file_hash.as_deref(),
                    track.audio_hash.as_deref(),
                    fingerprint,
                )?;
                hashes_updated += 1;
            }
        }

        let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for track in tracks {
            if let Some(audio_hash) = track.audio_hash {
                groups
                    .entry(format!("audio:{audio_hash}"))
                    .or_default()
                    .push(track.path);
            }
        }

        let mut duplicate_groups = 0_usize;
        let mut tracks_merged = 0_usize;
        for mut paths in groups.into_values().filter(|paths| paths.len() > 1) {
            duplicate_groups += 1;
            paths.sort();
            let canonical = paths[0].clone();
            for duplicate in paths.into_iter().skip(1) {
                if store.merge_duplicate_track(&canonical, &duplicate)? {
                    tracks_merged += 1;
                }
            }
        }

        Ok(AuditSummary {
            tracks_scanned,
            hashes_updated,
            duplicate_groups,
            tracks_merged,
            failures,
        })
    }
}
