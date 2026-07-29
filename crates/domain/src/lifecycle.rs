#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackLifecycleAction {
    None,
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaybackLifecycle {
    interruption_active: bool,
    resume_after_interruption: bool,
}

impl PlaybackLifecycle {
    pub fn interruption_active(&self) -> bool {
        self.interruption_active
    }

    pub fn resume_after_interruption(&self) -> bool {
        self.resume_after_interruption
    }

    pub fn begin_interruption(&mut self, is_playing: bool) -> PlaybackLifecycleAction {
        if self.interruption_active {
            return PlaybackLifecycleAction::None;
        }

        self.interruption_active = true;
        self.resume_after_interruption = is_playing;
        if is_playing {
            PlaybackLifecycleAction::Pause
        } else {
            PlaybackLifecycleAction::None
        }
    }

    pub fn end_interruption(
        &mut self,
        system_should_resume: bool,
        has_current_track: bool,
    ) -> PlaybackLifecycleAction {
        if !self.interruption_active {
            return PlaybackLifecycleAction::None;
        }

        self.interruption_active = false;
        let should_resume =
            self.resume_after_interruption && system_should_resume && has_current_track;
        self.resume_after_interruption = false;
        if should_resume {
            PlaybackLifecycleAction::Resume
        } else {
            PlaybackLifecycleAction::None
        }
    }

    pub fn request_playback_start(&mut self) -> bool {
        if self.interruption_active {
            return false;
        }

        self.resume_after_interruption = false;
        true
    }

    pub fn user_stopped_playback(&mut self) {
        self.resume_after_interruption = false;
    }

    pub fn output_disconnected(&mut self, is_playing: bool) -> PlaybackLifecycleAction {
        self.resume_after_interruption = false;
        if is_playing {
            PlaybackLifecycleAction::Pause
        } else {
            PlaybackLifecycleAction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_pauses_and_only_resumes_when_the_system_allows_it() {
        let mut lifecycle = PlaybackLifecycle::default();

        assert_eq!(
            lifecycle.begin_interruption(true),
            PlaybackLifecycleAction::Pause
        );
        assert!(lifecycle.interruption_active());
        assert!(lifecycle.resume_after_interruption());
        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::Resume
        );
        assert!(!lifecycle.interruption_active());
        assert!(!lifecycle.resume_after_interruption());
    }

    #[test]
    fn interruption_does_not_resume_a_track_that_was_already_paused() {
        let mut lifecycle = PlaybackLifecycle::default();

        assert_eq!(
            lifecycle.begin_interruption(false),
            PlaybackLifecycleAction::None
        );
        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::None
        );
    }

    #[test]
    fn interruption_does_not_resume_without_system_permission_or_a_track() {
        let mut lifecycle = PlaybackLifecycle::default();
        lifecycle.begin_interruption(true);
        assert_eq!(
            lifecycle.end_interruption(false, true),
            PlaybackLifecycleAction::None
        );

        lifecycle.begin_interruption(true);
        assert_eq!(
            lifecycle.end_interruption(true, false),
            PlaybackLifecycleAction::None
        );
    }

    #[test]
    fn repeated_interruption_notifications_are_idempotent() {
        let mut lifecycle = PlaybackLifecycle::default();

        assert_eq!(
            lifecycle.begin_interruption(true),
            PlaybackLifecycleAction::Pause
        );
        assert_eq!(
            lifecycle.begin_interruption(false),
            PlaybackLifecycleAction::None
        );
        assert!(lifecycle.resume_after_interruption());
        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::Resume
        );
        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::None
        );
    }

    #[test]
    fn explicit_user_action_cancels_automatic_resume() {
        let mut lifecycle = PlaybackLifecycle::default();
        lifecycle.begin_interruption(true);

        lifecycle.user_stopped_playback();

        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::None
        );
    }

    #[test]
    fn playback_cannot_start_while_an_interruption_is_active() {
        let mut lifecycle = PlaybackLifecycle::default();
        lifecycle.begin_interruption(true);

        assert!(!lifecycle.request_playback_start());
        assert!(lifecycle.interruption_active());
        assert!(lifecycle.resume_after_interruption());

        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::Resume
        );
        assert!(lifecycle.request_playback_start());
    }

    #[test]
    fn playback_can_start_when_there_is_no_active_interruption() {
        let mut lifecycle = PlaybackLifecycle::default();

        assert!(lifecycle.request_playback_start());
        assert!(!lifecycle.interruption_active());
        assert!(!lifecycle.resume_after_interruption());
    }

    #[test]
    fn disconnecting_an_output_pauses_without_scheduling_a_resume() {
        let mut lifecycle = PlaybackLifecycle::default();
        lifecycle.begin_interruption(true);

        assert_eq!(
            lifecycle.output_disconnected(true),
            PlaybackLifecycleAction::Pause
        );
        assert!(!lifecycle.resume_after_interruption());
        assert_eq!(
            lifecycle.end_interruption(true, true),
            PlaybackLifecycleAction::None
        );
    }
}
