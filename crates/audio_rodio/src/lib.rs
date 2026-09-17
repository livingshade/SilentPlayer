use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use domain::{GainDecision, Track};
use engine::{AudioBackend, AudioRenderSettings};
use errors::{PlayerError, PlayerResult};
use rodio::cpal::traits::{DeviceTrait, HostTrait};

const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub struct RodioBackend {
    stream: rodio::MixerDeviceSink,
    player: Option<rodio::Player>,
    watch: Arc<DeviceWatch>,
}

struct DeviceWatch {
    stream_failed: AtomicBool,
    device_changed: AtomicBool,
    opened_device_id: Mutex<String>,
}

impl DeviceWatch {
    fn new(opened_device_id: String) -> Self {
        Self {
            stream_failed: AtomicBool::new(false),
            device_changed: AtomicBool::new(false),
            opened_device_id: Mutex::new(opened_device_id),
        }
    }

    fn note_stream_error(&self) {
        self.stream_failed.store(true, Ordering::SeqCst);
    }

    fn observe_default_device(&self, observed: Option<&str>) {
        if let Some(observed) = observed {
            let opened = self.opened_device_id.lock().unwrap();
            if observed != opened.as_str() {
                self.device_changed.store(true, Ordering::SeqCst);
            }
        }
    }

    fn needs_reopen(&self) -> bool {
        self.stream_failed.load(Ordering::SeqCst) || self.device_changed.load(Ordering::SeqCst)
    }
}

fn default_output_device_id() -> Option<String> {
    rodio::cpal::default_host()
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|device_id| device_id.to_string())
}

fn open_sink(watch: &Arc<DeviceWatch>) -> PlayerResult<rodio::MixerDeviceSink> {
    let error_watch = Arc::clone(watch);
    let opened = rodio::DeviceSinkBuilder::from_default_device().and_then(|builder| {
        builder
            .with_error_callback(move |_error| error_watch.note_stream_error())
            .open_sink_or_fallback()
    });
    match opened {
        Ok(stream) => Ok(stream),
        Err(_) => rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| PlayerError::audio(error.to_string())),
    }
}

fn spawn_device_watcher(watch: &Arc<DeviceWatch>) {
    let watch = Arc::downgrade(watch);
    let _ = std::thread::Builder::new()
        .name("audio-device-watch".to_owned())
        .spawn(move || loop {
            std::thread::sleep(DEVICE_POLL_INTERVAL);
            let Some(watch) = watch.upgrade() else {
                return;
            };
            let observed = default_output_device_id();
            watch.observe_default_device(observed.as_deref());
        });
}

impl RodioBackend {
    pub fn open_default() -> PlayerResult<Self> {
        let opened_device_id = default_output_device_id().unwrap_or_default();
        let watch = Arc::new(DeviceWatch::new(opened_device_id));
        let mut stream = open_sink(&watch)?;
        stream.log_on_drop(false);
        spawn_device_watcher(&watch);

        Ok(Self {
            stream,
            player: None,
            watch,
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

    fn stream_failed(&self) -> bool {
        self.watch.needs_reopen()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_flags_device_change_as_reopen_needed() {
        let watch = DeviceWatch::new("Built-in Output".to_owned());

        watch.observe_default_device(Some("Built-in Output"));
        assert!(!watch.needs_reopen());

        watch.observe_default_device(Some("AirPods Pro"));
        assert!(watch.needs_reopen());
    }

    #[test]
    fn watch_ignores_a_missing_default_device() {
        let watch = DeviceWatch::new("Built-in Output".to_owned());

        watch.observe_default_device(None);

        assert!(!watch.needs_reopen());
    }

    #[test]
    fn watch_flags_stream_errors_as_reopen_needed() {
        let watch = DeviceWatch::new("Built-in Output".to_owned());

        assert!(!watch.needs_reopen());
        watch.note_stream_error();
        assert!(watch.needs_reopen());
    }
}
