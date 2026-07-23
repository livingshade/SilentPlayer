use std::fs::File;
use std::time::Duration;

use player_core::{GainDecision, Track};
use player_engine::{AudioBackend, AudioRenderSettings};
use player_error::{PlayerError, PlayerResult};

pub struct RodioBackend {
    stream: rodio::MixerDeviceSink,
    player: Option<rodio::Player>,
}

impl RodioBackend {
    pub fn open_default() -> PlayerResult<Self> {
        let mut stream = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| PlayerError::audio(error.to_string()))?;
        stream.log_on_drop(false);

        Ok(Self {
            stream,
            player: None,
        })
    }

    pub fn play_track_blocking(
        &mut self,
        track: &Track,
        settings: AudioRenderSettings,
        stop_after: Option<Duration>,
    ) -> PlayerResult<()> {
        self.load(track, settings)?;
        self.play()?;

        if let Some(duration) = stop_after {
            std::thread::sleep(duration);
            self.pause()?;
            return Ok(());
        }

        let Some(player) = &self.player else {
            return Err(PlayerError::audio("no player after load"));
        };
        player.sleep_until_end();
        Ok(())
    }

    fn current_player(&self) -> PlayerResult<&rodio::Player> {
        self.player
            .as_ref()
            .ok_or_else(|| PlayerError::audio("no track loaded"))
    }
}

impl AudioBackend for RodioBackend {
    fn load(&mut self, track: &Track, settings: AudioRenderSettings) -> PlayerResult<()> {
        let file = File::open(&track.path)
            .map_err(|source| PlayerError::io(track.path.clone(), source))?;
        let decoder = rodio::Decoder::try_from(file)
            .map_err(|error| PlayerError::audio(error.to_string()))?;

        let player = rodio::Player::connect_new(self.stream.mixer());
        player.set_volume(settings.gain.linear_gain);
        player.append(decoder);

        if settings.start_position_ms > 0 {
            player
                .try_seek(Duration::from_millis(settings.start_position_ms))
                .map_err(|error| PlayerError::audio(error.to_string()))?;
        }

        self.player = Some(player);
        Ok(())
    }

    fn play(&mut self) -> PlayerResult<()> {
        self.current_player()?.play();
        Ok(())
    }

    fn pause(&mut self) -> PlayerResult<()> {
        self.current_player()?.pause();
        Ok(())
    }

    fn seek_to(&mut self, position_ms: u64) -> PlayerResult<()> {
        self.current_player()?
            .try_seek(Duration::from_millis(position_ms))
            .map_err(|error| PlayerError::audio(error.to_string()))
    }

    fn set_gain(&mut self, gain: GainDecision) -> PlayerResult<()> {
        self.current_player()?.set_volume(gain.linear_gain);
        Ok(())
    }

    fn position_ms(&self) -> PlayerResult<u64> {
        let millis = self.current_player()?.get_pos().as_millis();
        Ok(millis.min(u128::from(u64::MAX)) as u64)
    }

    fn is_finished(&self) -> PlayerResult<bool> {
        Ok(self.current_player()?.empty())
    }
}
