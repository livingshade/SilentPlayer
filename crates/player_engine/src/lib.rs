use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use player_core::{
    GainDecision, NormalizationSettings, PlaybackCommand, PlaybackError, PlaybackState,
    PlayerSession, RepeatMode, Track, TrackId,
};
use player_error::{PlayerError, PlayerResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioRenderSettings {
    pub start_position_ms: u64,
    pub gain: GainDecision,
}

impl AudioRenderSettings {
    pub fn new(start_position_ms: u64, gain: GainDecision) -> Self {
        Self {
            start_position_ms,
            gain,
        }
    }
}

pub trait AudioBackend {
    fn load(&mut self, track: &Track, settings: AudioRenderSettings) -> PlayerResult<()>;
    fn play(&mut self) -> PlayerResult<()>;
    fn pause(&mut self) -> PlayerResult<()>;
    fn seek_to(&mut self, position_ms: u64) -> PlayerResult<()>;
    fn set_gain(&mut self, gain: GainDecision) -> PlayerResult<()>;
    fn position_ms(&self) -> PlayerResult<u64>;
    fn is_finished(&self) -> PlayerResult<bool> {
        Ok(false)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum EngineCommand {
    LoadQueue {
        queue: Vec<Track>,
        start_index: usize,
    },
    PlayQueue {
        queue: Vec<Track>,
        start_index: usize,
        repeat_mode: RepeatMode,
        shuffle: bool,
    },
    Play,
    Pause,
    Next,
    Previous,
    SeekTo {
        position_ms: u64,
    },
    SetRepeatMode(RepeatMode),
    SetShuffle(bool),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaybackEvent {
    StateChanged(PlaybackState),
    TrackChanged(Option<Box<Track>>),
    GainChanged(Option<GainDecision>),
    PositionChanged(u64),
    Error(String),
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineOptions {
    pub normalization: NormalizationSettings,
    pub poll_interval: Duration,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            normalization: NormalizationSettings::default(),
            poll_interval: Duration::from_millis(250),
        }
    }
}

pub struct PlayerEngine {
    requests: Sender<EngineRequest>,
    events: Receiver<PlaybackEvent>,
    handle: Option<JoinHandle<()>>,
}

struct EngineRequest {
    command: EngineCommand,
    completion: Sender<PlayerResult<()>>,
}

impl PlayerEngine {
    pub fn spawn<B, F>(
        normalization: NormalizationSettings,
        backend_factory: F,
    ) -> PlayerResult<Self>
    where
        B: AudioBackend + Send + 'static,
        F: FnOnce() -> PlayerResult<B> + Send + 'static,
    {
        Self::spawn_with_options(
            EngineOptions {
                normalization,
                ..EngineOptions::default()
            },
            backend_factory,
        )
    }

    pub fn spawn_with_options<B, F>(
        options: EngineOptions,
        backend_factory: F,
    ) -> PlayerResult<Self>
    where
        B: AudioBackend + Send + 'static,
        F: FnOnce() -> PlayerResult<B> + Send + 'static,
    {
        let (request_tx, request_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let handle = thread::Builder::new()
            .name("player-engine".to_owned())
            .spawn(move || {
                run_engine(
                    options.normalization,
                    options.poll_interval,
                    backend_factory,
                    request_rx,
                    event_tx,
                )
            })
            .map_err(|error| PlayerError::engine(error.to_string()))?;

        Ok(Self {
            requests: request_tx,
            events: event_rx,
            handle: Some(handle),
        })
    }

    pub fn load_queue(&self, queue: Vec<Track>, start_index: usize) -> PlayerResult<()> {
        self.execute(EngineCommand::LoadQueue { queue, start_index })
    }

    pub fn play_queue(
        &self,
        queue: Vec<Track>,
        start_index: usize,
        repeat_mode: RepeatMode,
        shuffle: bool,
    ) -> PlayerResult<()> {
        self.execute(EngineCommand::PlayQueue {
            queue,
            start_index,
            repeat_mode,
            shuffle,
        })
    }

    pub fn play(&self) -> PlayerResult<()> {
        self.execute(EngineCommand::Play)
    }

    pub fn pause(&self) -> PlayerResult<()> {
        self.execute(EngineCommand::Pause)
    }

    pub fn next(&self) -> PlayerResult<()> {
        self.execute(EngineCommand::Next)
    }

    pub fn previous(&self) -> PlayerResult<()> {
        self.execute(EngineCommand::Previous)
    }

    pub fn seek_to(&self, position_ms: u64) -> PlayerResult<()> {
        self.execute(EngineCommand::SeekTo { position_ms })
    }

    pub fn set_repeat_mode(&self, repeat_mode: RepeatMode) -> PlayerResult<()> {
        self.execute(EngineCommand::SetRepeatMode(repeat_mode))
    }

    pub fn set_shuffle(&self, enabled: bool) -> PlayerResult<()> {
        self.execute(EngineCommand::SetShuffle(enabled))
    }

    pub fn recv_event_timeout(&self, timeout: Duration) -> Result<PlaybackEvent, RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }

    pub fn try_recv_event(&self) -> Option<PlaybackEvent> {
        self.events.try_recv().ok()
    }

    pub fn shutdown(mut self) -> PlayerResult<()> {
        self.execute(EngineCommand::Shutdown)?;
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| PlayerError::engine("engine thread panicked"))?;
        }
        Ok(())
    }

    fn execute(&self, command: EngineCommand) -> PlayerResult<()> {
        let (completion_tx, completion_rx) = mpsc::channel();
        self.requests
            .send(EngineRequest {
                command,
                completion: completion_tx,
            })
            .map_err(|error| PlayerError::engine(error.to_string()))?;
        completion_rx
            .recv()
            .map_err(|error| PlayerError::engine(error.to_string()))?
    }
}

impl Drop for PlayerEngine {
    fn drop(&mut self) {
        let (completion, _) = mpsc::channel();
        let _ = self.requests.send(EngineRequest {
            command: EngineCommand::Shutdown,
            completion,
        });
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_engine<B, F>(
    normalization: NormalizationSettings,
    poll_interval: Duration,
    backend_factory: F,
    request_rx: Receiver<EngineRequest>,
    event_tx: Sender<PlaybackEvent>,
) where
    B: AudioBackend + Send + 'static,
    F: FnOnce() -> PlayerResult<B>,
{
    let mut backend = match backend_factory() {
        Ok(backend) => backend,
        Err(error) => {
            let _ = event_tx.send(PlaybackEvent::Error(error.to_string()));
            let _ = event_tx.send(PlaybackEvent::Stopped);
            return;
        }
    };
    let mut session = PlayerSession::new(normalization);
    let mut loaded_track_id = None;

    loop {
        let result = match request_rx.recv_timeout(poll_interval) {
            Ok(EngineRequest {
                command: EngineCommand::Shutdown,
                completion,
            }) => {
                let _ = event_tx.send(PlaybackEvent::Stopped);
                let _ = completion.send(Ok(()));
                break;
            }
            Ok(request) => {
                let result = handle_command(
                    request.command,
                    &mut session,
                    &mut backend,
                    &mut loaded_track_id,
                );
                match result {
                    Ok(()) => {
                        publish_snapshot(&mut session, &backend, &event_tx);
                        let _ = request.completion.send(Ok(()));
                    }
                    Err(error) => {
                        let _ = event_tx.send(PlaybackEvent::Error(error.to_string()));
                        publish_snapshot(&mut session, &backend, &event_tx);
                        let _ = request.completion.send(Err(error));
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {
                poll_playback(&mut session, &mut backend, &mut loaded_track_id)
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };

        match result {
            Ok(true) => publish_snapshot(&mut session, &backend, &event_tx),
            Ok(false) => {}
            Err(error) => {
                let _ = event_tx.send(PlaybackEvent::Error(error.to_string()));
                publish_snapshot(&mut session, &backend, &event_tx);
            }
        }
    }
}

fn handle_command<B: AudioBackend>(
    command: EngineCommand,
    session: &mut PlayerSession,
    backend: &mut B,
    loaded_track_id: &mut Option<TrackId>,
) -> PlayerResult<()> {
    match command {
        EngineCommand::LoadQueue { queue, start_index } => {
            *loaded_track_id = None;
            session
                .set_queue(queue, start_index)
                .map_err(player_error_from_playback)?;
            if session.state().is_playing && session.current_track().is_some() {
                load_current_if_needed(session, backend, loaded_track_id)?;
                backend.play()?;
            }
            Ok(())
        }
        EngineCommand::PlayQueue {
            queue,
            start_index,
            repeat_mode,
            shuffle,
        } => {
            *loaded_track_id = None;
            session
                .set_queue(queue, start_index)
                .map_err(player_error_from_playback)?;
            session.set_repeat_mode(repeat_mode);
            session.set_shuffle(shuffle);
            start_playback(session, backend, loaded_track_id)
        }
        EngineCommand::Play => start_playback(session, backend, loaded_track_id),
        EngineCommand::Pause => {
            session
                .apply(PlaybackCommand::Pause)
                .map_err(player_error_from_playback)?;
            backend.pause()
        }
        EngineCommand::Next => {
            session
                .apply(PlaybackCommand::Next)
                .map_err(player_error_from_playback)?;
            *loaded_track_id = None;
            if session.current_track().is_some() {
                load_current_if_needed(session, backend, loaded_track_id)?;
                if session.state().is_playing {
                    backend.play()?;
                }
            }
            Ok(())
        }
        EngineCommand::Previous => {
            let before = session.current_track().map(|track| track.id);
            session
                .apply(PlaybackCommand::Previous)
                .map_err(player_error_from_playback)?;
            if session.current_track().map(|track| track.id) != before {
                *loaded_track_id = None;
                load_current_if_needed(session, backend, loaded_track_id)?;
                if session.state().is_playing {
                    backend.play()?;
                }
            } else {
                backend.seek_to(session.state().position_ms)?;
            }
            Ok(())
        }
        EngineCommand::SeekTo { position_ms } => {
            session
                .apply(PlaybackCommand::SeekTo { position_ms })
                .map_err(player_error_from_playback)?;
            backend.seek_to(position_ms)
        }
        EngineCommand::SetRepeatMode(repeat_mode) => {
            session.set_repeat_mode(repeat_mode);
            Ok(())
        }
        EngineCommand::SetShuffle(enabled) => {
            session.set_shuffle(enabled);
            Ok(())
        }
        EngineCommand::Shutdown => unreachable!("handled before command dispatch"),
    }
}

fn start_playback<B: AudioBackend>(
    session: &mut PlayerSession,
    backend: &mut B,
    loaded_track_id: &mut Option<TrackId>,
) -> PlayerResult<()> {
    session
        .apply(PlaybackCommand::Play)
        .map_err(player_error_from_playback)?;
    if let Err(error) =
        load_current_if_needed(session, backend, loaded_track_id).and_then(|()| backend.play())
    {
        session
            .apply(PlaybackCommand::Pause)
            .map_err(player_error_from_playback)?;
        return Err(error);
    }
    Ok(())
}

fn poll_playback<B: AudioBackend>(
    session: &mut PlayerSession,
    backend: &mut B,
    loaded_track_id: &mut Option<TrackId>,
) -> PlayerResult<bool> {
    if !session.state().is_playing {
        return Ok(false);
    }

    if backend.is_finished()? {
        session
            .apply(PlaybackCommand::Next)
            .map_err(player_error_from_playback)?;
        *loaded_track_id = None;
        if session.current_track().is_some() {
            load_current_if_needed(session, backend, loaded_track_id)?;
            backend.play()?;
        }
    }

    Ok(true)
}

fn load_current_if_needed<B: AudioBackend>(
    session: &PlayerSession,
    backend: &mut B,
    loaded_track_id: &mut Option<TrackId>,
) -> PlayerResult<()> {
    let Some(track) = session.current_track() else {
        return Err(PlayerError::invalid_input("queue is empty"));
    };

    if *loaded_track_id == Some(track.id) {
        return Ok(());
    }

    let gain = session
        .current_gain()
        .unwrap_or_else(|| GainDecision::unity(player_core::LoudnessStatus::NeedsAnalysis));
    backend.load(
        track,
        AudioRenderSettings::new(session.state().position_ms, gain),
    )?;
    *loaded_track_id = Some(track.id);
    Ok(())
}

fn player_error_from_playback(error: PlaybackError) -> PlayerError {
    PlayerError::invalid_input(error.to_string())
}

fn publish_snapshot<B: AudioBackend>(
    session: &mut PlayerSession,
    backend: &B,
    event_tx: &Sender<PlaybackEvent>,
) {
    let position_ms = if session.current_track().is_some() {
        backend.position_ms().ok()
    } else {
        None
    };
    if let Some(position_ms) = position_ms {
        let _ = session.apply(PlaybackCommand::SeekTo { position_ms });
    }

    let _ = event_tx.send(PlaybackEvent::StateChanged(session.state().clone()));
    let _ = event_tx.send(PlaybackEvent::TrackChanged(
        session.current_track().cloned().map(Box::new),
    ));
    let _ = event_tx.send(PlaybackEvent::GainChanged(session.current_gain()));
    if let Some(position_ms) = position_ms {
        let _ = event_tx.send(PlaybackEvent::PositionChanged(position_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug)]
    struct Calls(Arc<Mutex<Vec<String>>>);

    impl Calls {
        fn push(&self, value: impl Into<String>) {
            self.0.lock().unwrap().push(value.into());
        }

        fn values(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    struct FakeBackend {
        calls: Calls,
        position_ms: Arc<AtomicU64>,
        finished_sequence: Arc<Mutex<Vec<bool>>>,
    }

    struct FailingPlayBackend;

    impl AudioBackend for FailingPlayBackend {
        fn load(&mut self, _track: &Track, _settings: AudioRenderSettings) -> PlayerResult<()> {
            Ok(())
        }

        fn play(&mut self) -> PlayerResult<()> {
            Err(PlayerError::audio("play failed"))
        }

        fn pause(&mut self) -> PlayerResult<()> {
            Ok(())
        }

        fn seek_to(&mut self, _position_ms: u64) -> PlayerResult<()> {
            Ok(())
        }

        fn set_gain(&mut self, _gain: GainDecision) -> PlayerResult<()> {
            Ok(())
        }

        fn position_ms(&self) -> PlayerResult<u64> {
            Ok(0)
        }

        fn is_finished(&self) -> PlayerResult<bool> {
            Ok(false)
        }
    }

    impl AudioBackend for FakeBackend {
        fn load(&mut self, track: &Track, settings: AudioRenderSettings) -> PlayerResult<()> {
            self.position_ms
                .store(settings.start_position_ms, Ordering::SeqCst);
            self.calls
                .push(format!("load:{}:{:.2}", track.title, settings.gain.gain_db));
            Ok(())
        }

        fn play(&mut self) -> PlayerResult<()> {
            self.calls.push("play");
            Ok(())
        }

        fn pause(&mut self) -> PlayerResult<()> {
            self.calls.push("pause");
            Ok(())
        }

        fn seek_to(&mut self, position_ms: u64) -> PlayerResult<()> {
            self.position_ms.store(position_ms, Ordering::SeqCst);
            self.calls.push(format!("seek:{position_ms}"));
            Ok(())
        }

        fn set_gain(&mut self, gain: GainDecision) -> PlayerResult<()> {
            self.calls.push(format!("gain:{:.2}", gain.gain_db));
            Ok(())
        }

        fn position_ms(&self) -> PlayerResult<u64> {
            Ok(self.position_ms.fetch_add(50, Ordering::SeqCst))
        }

        fn is_finished(&self) -> PlayerResult<bool> {
            let mut sequence = self.finished_sequence.lock().unwrap();
            if sequence.is_empty() {
                Ok(false)
            } else {
                Ok(sequence.remove(0))
            }
        }
    }

    fn track(name: &str) -> Track {
        let mut track = Track::from_path(format!("{name}.ogg").into());
        track.loudness = Some(player_core::LoudnessInfo::track(-20.0, -8.0));
        track
    }

    #[test]
    fn engine_loads_and_plays_current_track() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let engine = PlayerEngine::spawn(NormalizationSettings::default(), move || {
            Ok(FakeBackend {
                calls: backend_calls,
                position_ms: Arc::new(AtomicU64::new(0)),
                finished_sequence: Arc::new(Mutex::new(Vec::new())),
            })
        })
        .unwrap();

        engine.load_queue(vec![track("a"), track("b")], 0).unwrap();
        engine.play().unwrap();
        assert!(calls.values().iter().any(|value| value == "play"));
        engine.shutdown().unwrap();

        let values = calls.values();
        assert!(values.iter().any(|value| value == "load:a:4.00"));
        assert!(values.iter().any(|value| value == "play"));
    }

    #[test]
    fn play_queue_applies_modes_and_acknowledges_backend_start_atomically() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let engine = PlayerEngine::spawn(NormalizationSettings::default(), move || {
            Ok(FakeBackend {
                calls: backend_calls,
                position_ms: Arc::new(AtomicU64::new(0)),
                finished_sequence: Arc::new(Mutex::new(Vec::new())),
            })
        })
        .unwrap();

        engine
            .play_queue(vec![track("a"), track("b")], 1, RepeatMode::All, true)
            .unwrap();

        assert!(calls.values().iter().any(|value| value == "load:b:4.00"));
        assert!(calls.values().iter().any(|value| value == "play"));
        wait_for_event(&engine, |event| {
            matches!(event, PlaybackEvent::StateChanged(state)
                if state.is_playing && state.current_index == Some(1)
                    && state.repeat_mode == RepeatMode::All && state.shuffle)
        });
        engine.shutdown().unwrap();
    }

    #[test]
    fn engine_rolls_back_playing_state_when_backend_play_fails() {
        let engine =
            PlayerEngine::spawn(NormalizationSettings::default(), || Ok(FailingPlayBackend))
                .unwrap();

        engine.load_queue(vec![track("a")], 0).unwrap();
        let error = engine.play().unwrap_err();
        assert!(error.to_string().contains("play failed"));

        let event = wait_for_event(&engine, |event| matches!(event, PlaybackEvent::Error(_)));
        assert!(matches!(event, PlaybackEvent::Error(message) if message.contains("play failed")));
        wait_for_event(
            &engine,
            |event| matches!(event, PlaybackEvent::StateChanged(state) if !state.is_playing),
        );
        engine.shutdown().unwrap();
    }

    #[test]
    fn engine_next_loads_next_track() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let engine = PlayerEngine::spawn(NormalizationSettings::default(), move || {
            Ok(FakeBackend {
                calls: backend_calls,
                position_ms: Arc::new(AtomicU64::new(0)),
                finished_sequence: Arc::new(Mutex::new(Vec::new())),
            })
        })
        .unwrap();

        engine.load_queue(vec![track("a"), track("b")], 0).unwrap();
        engine.play().unwrap();
        wait_for_call(&calls, "play");
        engine.next().unwrap();
        wait_for_call(&calls, "load:b:4.00");
        engine.shutdown().unwrap();

        let values = calls.values();
        assert!(values.iter().any(|value| value == "load:a:4.00"));
        assert!(values.iter().any(|value| value == "load:b:4.00"));
    }

    #[test]
    fn engine_publishes_progress_while_playing() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let engine = PlayerEngine::spawn_with_options(
            EngineOptions {
                poll_interval: Duration::from_millis(10),
                ..EngineOptions::default()
            },
            move || {
                Ok(FakeBackend {
                    calls: backend_calls,
                    position_ms: Arc::new(AtomicU64::new(0)),
                    finished_sequence: Arc::new(Mutex::new(Vec::new())),
                })
            },
        )
        .unwrap();

        engine.load_queue(vec![track("a")], 0).unwrap();
        engine.play().unwrap();
        wait_for_event(
            &engine,
            |event| matches!(event, PlaybackEvent::PositionChanged(position_ms) if position_ms > 0),
        );
        engine.shutdown().unwrap();

        assert!(calls.values().iter().any(|value| value == "play"));
    }

    #[test]
    fn engine_advances_to_next_track_when_backend_finishes() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let backend_finished_sequence = Arc::new(Mutex::new(vec![true]));
        let engine = PlayerEngine::spawn_with_options(
            EngineOptions {
                poll_interval: Duration::from_millis(10),
                ..EngineOptions::default()
            },
            move || {
                Ok(FakeBackend {
                    calls: backend_calls,
                    position_ms: Arc::new(AtomicU64::new(0)),
                    finished_sequence: backend_finished_sequence,
                })
            },
        )
        .unwrap();

        engine.load_queue(vec![track("a"), track("b")], 0).unwrap();
        engine.play().unwrap();
        wait_for_call(&calls, "load:b:4.00");
        engine.shutdown().unwrap();

        let values = calls.values();
        assert!(values.iter().any(|value| value == "load:a:4.00"));
        assert!(values.iter().any(|value| value == "load:b:4.00"));
        assert!(values.iter().filter(|value| *value == "play").count() >= 2);
    }

    #[test]
    fn engine_updates_repeat_and_shuffle_state() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let engine = PlayerEngine::spawn_with_options(
            EngineOptions {
                poll_interval: Duration::from_millis(10),
                ..EngineOptions::default()
            },
            move || {
                Ok(FakeBackend {
                    calls: backend_calls,
                    position_ms: Arc::new(AtomicU64::new(0)),
                    finished_sequence: Arc::new(Mutex::new(Vec::new())),
                })
            },
        )
        .unwrap();

        engine.set_shuffle(true).unwrap();
        engine.set_repeat_mode(RepeatMode::One).unwrap();

        wait_for_event(
            &engine,
            |event| matches!(event, PlaybackEvent::StateChanged(state) if state.shuffle),
        );
        wait_for_event(
            &engine,
            |event| matches!(event, PlaybackEvent::StateChanged(state) if state.repeat_mode == RepeatMode::One),
        );
        engine.shutdown().unwrap();

        assert!(calls.values().is_empty());
    }

    #[test]
    fn engine_repeat_one_reloads_same_track_when_backend_finishes() {
        let calls = Calls(Arc::new(Mutex::new(Vec::new())));
        let backend_calls = calls.clone();
        let backend_finished_sequence = Arc::new(Mutex::new(vec![true]));
        let engine = PlayerEngine::spawn_with_options(
            EngineOptions {
                poll_interval: Duration::from_millis(10),
                ..EngineOptions::default()
            },
            move || {
                Ok(FakeBackend {
                    calls: backend_calls,
                    position_ms: Arc::new(AtomicU64::new(0)),
                    finished_sequence: backend_finished_sequence,
                })
            },
        )
        .unwrap();

        engine.load_queue(vec![track("a"), track("b")], 0).unwrap();
        engine.set_repeat_mode(RepeatMode::One).unwrap();
        engine.play().unwrap();
        wait_for_call(&calls, "play");
        wait_for_repeated_call(&calls, "load:a:4.00", 2);
        engine.shutdown().unwrap();

        let values = calls.values();
        assert!(!values.iter().any(|value| value == "load:b:4.00"));
        assert!(values.iter().filter(|value| *value == "play").count() >= 2);
    }

    fn wait_for_call(calls: &Calls, expected: &str) {
        for _ in 0..50 {
            if calls.values().iter().any(|value| value == expected) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("missing call {expected:?}; calls={:?}", calls.values());
    }

    fn wait_for_repeated_call(calls: &Calls, expected: &str, count: usize) {
        for _ in 0..50 {
            if calls
                .values()
                .iter()
                .filter(|value| value.as_str() == expected)
                .count()
                >= count
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "missing {count} occurrences of {expected:?}; calls={:?}",
            calls.values()
        );
    }

    fn wait_for_event(
        engine: &PlayerEngine,
        predicate: impl Fn(PlaybackEvent) -> bool,
    ) -> PlaybackEvent {
        for _ in 0..50 {
            match engine.recv_event_timeout(Duration::from_millis(20)) {
                Ok(event) if predicate(event.clone()) => return event,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(error) => panic!("event channel closed: {error}"),
            }
        }
        panic!("missing expected engine event");
    }
}
