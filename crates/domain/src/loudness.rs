use crate::model::{LoudnessInfo, Track};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizationMode {
    Off,
    Track,
    Album,
    Smart,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizationSettings {
    pub mode: NormalizationMode,
    pub target_lufs: f32,
    pub true_peak_ceiling_dbtp: f32,
    pub user_preamp_db: f32,
    pub prevent_clipping: bool,
    pub max_boost_db: f32,
}

impl Default for NormalizationSettings {
    fn default() -> Self {
        Self {
            mode: NormalizationMode::Track,
            target_lufs: -16.0,
            true_peak_ceiling_dbtp: -1.0,
            user_preamp_db: 0.0,
            prevent_clipping: true,
            max_boost_db: 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoudnessStatus {
    Disabled,
    Ready,
    NeedsAnalysis,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainDecision {
    pub status: LoudnessStatus,
    pub gain_db: f32,
    pub linear_gain: f32,
    pub clipping_limited: bool,
}

impl GainDecision {
    pub fn unity(status: LoudnessStatus) -> Self {
        Self {
            status,
            gain_db: 0.0,
            linear_gain: 1.0,
            clipping_limited: false,
        }
    }

    pub fn ready(gain_db: f32, clipping_limited: bool) -> Self {
        Self {
            status: LoudnessStatus::Ready,
            gain_db,
            linear_gain: db_to_linear(gain_db),
            clipping_limited,
        }
    }
}

pub fn gain_for_track(track: &Track, settings: NormalizationSettings) -> GainDecision {
    if settings.mode == NormalizationMode::Off {
        return GainDecision::unity(LoudnessStatus::Disabled);
    }

    let Some(loudness) = &track.loudness else {
        return GainDecision::unity(LoudnessStatus::NeedsAnalysis);
    };

    gain_for_loudness(loudness, settings)
}

pub fn gain_for_loudness(loudness: &LoudnessInfo, settings: NormalizationSettings) -> GainDecision {
    if settings.mode == NormalizationMode::Off {
        return GainDecision::unity(LoudnessStatus::Disabled);
    }

    let source_lufs = match settings.mode {
        NormalizationMode::Off => unreachable!("handled above"),
        NormalizationMode::Track => loudness.integrated_lufs,
        NormalizationMode::Album => loudness
            .album_integrated_lufs
            .unwrap_or(loudness.integrated_lufs),
        NormalizationMode::Smart => loudness
            .album_integrated_lufs
            .unwrap_or(loudness.integrated_lufs),
    };

    let peak_dbtp = match settings.mode {
        NormalizationMode::Album | NormalizationMode::Smart => loudness
            .album_true_peak_dbtp
            .unwrap_or(loudness.true_peak_dbtp),
        NormalizationMode::Off | NormalizationMode::Track => loudness.true_peak_dbtp,
    };

    let desired_gain = settings.target_lufs - source_lufs + settings.user_preamp_db;
    let boost_limited_gain = desired_gain.min(settings.max_boost_db);

    let max_gain_without_clipping = settings.true_peak_ceiling_dbtp - peak_dbtp;
    let (gain_db, clipping_limited) =
        if settings.prevent_clipping && boost_limited_gain > max_gain_without_clipping {
            (max_gain_without_clipping, true)
        } else {
            (boost_limited_gain, false)
        };

    GainDecision {
        status: LoudnessStatus::Ready,
        gain_db,
        linear_gain: db_to_linear(gain_db),
        clipping_limited,
    }
}

pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_track_gain() {
        let loudness = LoudnessInfo::track(-20.0, -6.0);
        let settings = NormalizationSettings::default();
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.status, LoudnessStatus::Ready);
        assert_eq!(gain.gain_db, 4.0);
        assert!((gain.linear_gain - 1.5848932).abs() < 0.0001);
    }

    #[test]
    fn limits_gain_to_prevent_clipping() {
        let loudness = LoudnessInfo::track(-24.0, -2.0);
        let settings = NormalizationSettings::default();
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.gain_db, 1.0);
        assert!(gain.clipping_limited);
    }

    #[test]
    fn can_disable_clipping_prevention() {
        let loudness = LoudnessInfo::track(-24.0, -2.0);
        let settings = NormalizationSettings {
            prevent_clipping: false,
            ..NormalizationSettings::default()
        };
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.gain_db, 8.0);
        assert!(!gain.clipping_limited);
    }

    #[test]
    fn limits_excessive_boost() {
        let loudness = LoudnessInfo::track(-40.0, -40.0);
        let settings = NormalizationSettings {
            max_boost_db: 6.0,
            ..NormalizationSettings::default()
        };
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.gain_db, 6.0);
    }

    #[test]
    fn album_mode_uses_album_loudness_when_available() {
        let mut loudness = LoudnessInfo::track(-10.0, -8.0);
        loudness.album_integrated_lufs = Some(-20.0);
        loudness.album_true_peak_dbtp = Some(-12.0);

        let settings = NormalizationSettings {
            mode: NormalizationMode::Album,
            ..NormalizationSettings::default()
        };
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.gain_db, 4.0);
    }

    #[test]
    fn album_mode_falls_back_to_track_loudness() {
        let loudness = LoudnessInfo::track(-18.0, -10.0);
        let settings = NormalizationSettings {
            mode: NormalizationMode::Album,
            ..NormalizationSettings::default()
        };
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.gain_db, 2.0);
    }

    #[test]
    fn off_mode_returns_unity_gain() {
        let loudness = LoudnessInfo::track(-30.0, -30.0);
        let settings = NormalizationSettings {
            mode: NormalizationMode::Off,
            ..NormalizationSettings::default()
        };
        let gain = gain_for_loudness(&loudness, settings);
        assert_eq!(gain.status, LoudnessStatus::Disabled);
        assert_eq!(gain.gain_db, 0.0);
        assert_eq!(gain.linear_gain, 1.0);
    }

    #[test]
    fn returns_unity_when_missing_analysis() {
        let track = Track::from_path("song.mp3".into());
        let gain = gain_for_track(&track, NormalizationSettings::default());
        assert_eq!(gain.status, LoudnessStatus::NeedsAnalysis);
        assert_eq!(gain.linear_gain, 1.0);
    }
}
