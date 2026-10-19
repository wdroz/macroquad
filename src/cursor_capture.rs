//! Pointer-capture state shared by the desktop grab and the browser lock.
//!
//! The desktop path decides when to call the platform. The browser path only
//! mirrors the state reported by `js/mq_pointer_capture.js`.

/// Where pointer capture stands after [`crate::input::set_cursor_grab`].
///
/// ```
/// use macroquad::input::CursorCapture;
///
/// fn looking(state: CursorCapture) -> bool {
///     state == CursorCapture::Active
/// }
/// assert!(!looking(CursorCapture::Lost));
/// assert!(looking(CursorCapture::Active));
/// ```
///
/// # Desktop
///
/// [`crate::input::set_cursor_grab`]`(true)` is synchronous.
/// [`crate::input::cursor_capture`] is [`Active`](Self::Active) before the
/// call returns, and this frame's events are [`Requested`](Self::Requested)
/// then [`Active`](Self::Active). Miniquad does not report a failed grab, so
/// desktop code does not observe [`Denied`](Self::Denied). The call is the
/// gesture: after [`Lost`](Self::Lost), another `set_cursor_grab(true)` grabs
/// again. Focus loss (and Android pause, which uses the same minimized event)
/// releases the platform grab once and becomes [`Lost`](Self::Lost).
/// Regaining focus does not grab again.
///
/// # Browser
///
/// The bundled `mq_js_bundle.js` tracks the Pointer Lock promise. A request
/// without a user gesture becomes [`CursorCapture::Denied`]. Escape, a
/// rejected promise, and focus loss do not call `requestPointerLock` again.
/// While the last `set_cursor_grab(true)` still stands, the next primary
/// pointer or touch press asks once, inside that event. `set_cursor_grab(false)`
/// cancels that retry. Calling `set_cursor_grab(true)` on later frames does
/// not ask the browser again until one of those presses. An older loader
/// without this plugin keeps the previous grab flag and reports only
/// [`CursorCapture::Idle`] or [`CursorCapture::Active`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CursorCapture {
    /// `set_cursor_grab(false)` has released capture, or nothing has asked yet.
    Idle = 0,
    /// A grab was sent and the platform has not confirmed it.
    /// On desktop this is only an event; the state is already [`Active`](Self::Active)
    /// when `set_cursor_grab` returns. In the browser it lasts until the promise
    /// and `pointerlockchange` settle.
    Requested = 1,
    /// The cursor is captured. Relative motion is applied to the mouse position.
    Active = 2,
    /// The browser refused the latest request. Another pointer or touch press
    /// is required before a new request is sent. Desktop grabs do not use this.
    Denied = 3,
    /// Capture ended because the platform released it (Escape, unlock, or focus
    /// loss) while the application still wanted it. Focus returning does not
    /// capture again.
    Lost = 4,
}

impl CursorCapture {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Idle),
            1 => Some(Self::Requested),
            2 => Some(Self::Active),
            3 => Some(Self::Denied),
            4 => Some(Self::Lost),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CaptureController {
    state: CursorCapture,
    events: Vec<CursorCapture>,
    /// Last grab sent to the desktop platform. The browser does not use this.
    platform_grabbed: bool,
    platform: Vec<bool>,
}

impl CaptureController {
    pub(crate) fn new() -> Self {
        Self {
            state: CursorCapture::Idle,
            events: Vec::new(),
            platform_grabbed: false,
            platform: Vec::new(),
        }
    }

    pub(crate) fn state(&self) -> CursorCapture {
        self.state
    }

    pub(crate) fn events(&self) -> &[CursorCapture] {
        &self.events
    }

    pub(crate) fn clear_events(&mut self) {
        self.events.clear();
    }

    pub(crate) fn take_platform(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.platform)
    }

    /// Desktop `set_cursor_grab`. A `true` call is the recovery gesture.
    pub(crate) fn set_wanted(&mut self, wanted: bool) {
        if wanted {
            if self.state == CursorCapture::Active {
                return;
            }
            if !self.platform_grabbed {
                self.platform_grabbed = true;
                self.platform.push(true);
            }
            self.transition(CursorCapture::Requested);
            // The OS call is synchronous and does not report refusal.
            self.transition(CursorCapture::Active);
        } else {
            if self.platform_grabbed {
                self.platform_grabbed = false;
                self.platform.push(false);
            }
            self.transition(CursorCapture::Idle);
        }
    }

    /// Desktop focus change. `true` does not restore a grab.
    pub(crate) fn focus_changed(&mut self, focused: bool) {
        if focused
            || (self.state != CursorCapture::Active && self.state != CursorCapture::Requested)
        {
            return;
        }
        if self.platform_grabbed {
            self.platform_grabbed = false;
            self.platform.push(false);
        }
        self.transition(CursorCapture::Lost);
    }

    /// Copy a browser report. Duplicate reports do not emit another event.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn observe_browser(&mut self, reported: CursorCapture) {
        self.transition(reported);
    }

    /// Loader without the pointer-capture plugin: the grab flag is the state.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn legacy_set(&mut self, grabbed: bool) {
        self.transition(if grabbed {
            CursorCapture::Active
        } else {
            CursorCapture::Idle
        });
    }

    fn transition(&mut self, next: CursorCapture) {
        if self.state != next {
            self.state = next;
            self.events.push(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_grab_confirms_once_and_ignores_repeat_calls() {
        let mut capture = CaptureController::new();
        capture.set_wanted(true);
        assert_eq!(capture.state(), CursorCapture::Active);
        assert_eq!(
            capture.events(),
            &[CursorCapture::Requested, CursorCapture::Active]
        );
        assert_eq!(capture.take_platform(), vec![true]);

        capture.clear_events();
        capture.set_wanted(true);
        capture.set_wanted(true);
        assert_eq!(capture.state(), CursorCapture::Active);
        assert!(capture.events().is_empty());
        assert!(capture.take_platform().is_empty());
    }

    #[test]
    fn native_release_and_repeated_release_send_one_ungrab() {
        let mut capture = CaptureController::new();
        capture.set_wanted(true);
        capture.take_platform();
        capture.clear_events();

        capture.set_wanted(false);
        assert_eq!(capture.state(), CursorCapture::Idle);
        assert_eq!(capture.events(), &[CursorCapture::Idle]);
        assert_eq!(capture.take_platform(), vec![false]);

        capture.set_wanted(false);
        assert!(capture.take_platform().is_empty());
    }

    #[test]
    fn native_focus_loss_does_not_return_when_focus_returns() {
        let mut capture = CaptureController::new();
        capture.set_wanted(true);
        capture.take_platform();
        capture.clear_events();

        capture.focus_changed(false);
        capture.focus_changed(false);
        assert_eq!(capture.state(), CursorCapture::Lost);
        assert_eq!(capture.events(), &[CursorCapture::Lost]);
        assert_eq!(capture.take_platform(), vec![false]);

        capture.focus_changed(true);
        assert_eq!(capture.state(), CursorCapture::Lost);
        assert!(capture.take_platform().is_empty());
        assert_eq!(capture.events(), &[CursorCapture::Lost]);

        capture.clear_events();
        capture.set_wanted(true);
        assert_eq!(capture.state(), CursorCapture::Active);
        assert_eq!(
            capture.events(),
            &[CursorCapture::Requested, CursorCapture::Active]
        );
        assert_eq!(capture.take_platform(), vec![true]);
    }

    #[test]
    fn native_release_after_focus_loss_does_not_ungrab_again() {
        let mut capture = CaptureController::new();
        capture.set_wanted(true);
        capture.focus_changed(false);
        assert_eq!(capture.take_platform(), vec![true, false]);

        capture.set_wanted(false);
        assert_eq!(capture.state(), CursorCapture::Idle);
        assert!(capture.take_platform().is_empty());
    }

    #[test]
    fn browser_reports_record_one_event_per_change() {
        let mut capture = CaptureController::new();
        capture.observe_browser(CursorCapture::Requested);
        capture.observe_browser(CursorCapture::Requested);
        capture.observe_browser(CursorCapture::Denied);
        capture.observe_browser(CursorCapture::Denied);
        capture.observe_browser(CursorCapture::Lost);
        capture.observe_browser(CursorCapture::Active);
        capture.observe_browser(CursorCapture::Active);
        assert_eq!(
            capture.events(),
            &[
                CursorCapture::Requested,
                CursorCapture::Denied,
                CursorCapture::Lost,
                CursorCapture::Active,
            ]
        );
        let events = capture.events().to_vec();
        assert_eq!(
            capture.events(),
            events.as_slice(),
            "reading does not drain events"
        );
        capture.clear_events();
        assert!(capture.events().is_empty());
        assert!(capture.take_platform().is_empty());
        assert_eq!(CursorCapture::from_raw(3), Some(CursorCapture::Denied));
        assert_eq!(CursorCapture::from_raw(9), None);
    }
}
