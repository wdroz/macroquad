//! Keyboard focus bookkeeping, independent of rendering and text editing.

use super::{Id, KeyCode};

pub(crate) fn rect_visible(rect: crate::math::Rect, clip: Option<crate::math::Rect>) -> bool {
    let valid = |r: crate::math::Rect| {
        r.x.is_finite()
            && r.y.is_finite()
            && r.w.is_finite()
            && r.h.is_finite()
            && r.w > 0.
            && r.h > 0.
    };
    valid(rect)
        && match clip {
            Some(clip) => {
                valid(clip)
                    && rect.x.max(clip.x) < (rect.x + rect.w).min(clip.x + clip.w)
                    && rect.y.max(clip.y) < (rect.y + rect.h).min(clip.y + clip.h)
            }
            None => true,
        }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    id: Id,
    scope: Option<Id>,
}

#[derive(Default)]
pub(crate) struct KeyboardFocus {
    entries: Vec<Entry>,
    previous_ids: Vec<Id>,
    modal: Option<Id>,
    submitted_modal: Option<Id>,
    saved_focus: Option<Id>,
    tab: Option<bool>,
    activate: bool,
    held: [bool; 3],
    opening_keys: [bool; 3],
}

impl KeyboardFocus {
    fn key_index(key: KeyCode) -> Option<usize> {
        match key {
            KeyCode::Tab => Some(0),
            KeyCode::Enter => Some(1),
            KeyCode::Space => Some(2),
            _ => None,
        }
    }

    pub fn key_down(&mut self, key: KeyCode, shift: bool, ctrl: bool) {
        if let Some(index) = Self::key_index(key) {
            if !self.held[index] && !ctrl {
                if key == KeyCode::Tab {
                    self.tab = Some(shift);
                } else {
                    self.activate = true;
                }
            }
            self.held[index] = true;
        }
    }

    pub fn key_up(&mut self, key: KeyCode) {
        if let Some(index) = Self::key_index(key) {
            self.held[index] = false;
            self.opening_keys[index] = false;
        }
    }

    pub fn suppress_key(&self, key: KeyCode) -> bool {
        Self::key_index(key).is_some_and(|index| self.opening_keys[index])
    }

    pub fn allows(&self, scope: Option<Id>) -> bool {
        self.modal == scope
    }

    pub fn enter_modal(&mut self, id: Id, focus: &mut Option<Id>) {
        if self.modal != Some(id) {
            if self.modal.is_none() {
                self.saved_focus = *focus;
            }
            self.modal = Some(id);
            *focus = None;
            // The key that opened a popup belongs to its opener.
            self.tab = None;
            self.activate = false;
            self.opening_keys = self.held;
        }
        self.submitted_modal = Some(id);
    }

    pub fn register(
        &mut self,
        id: Id,
        scope: Option<Id>,
        enabled: bool,
        focus: &mut Option<Id>,
    ) -> bool {
        if enabled {
            let entry = Entry { id, scope };
            if !self.entries.contains(&entry) {
                self.entries.push(entry);
            }
            if scope.is_some() && self.allows(scope) && focus.is_none() {
                *focus = Some(id);
            }
        } else if *focus == Some(id) && self.allows(scope) {
            *focus = None;
            // A disabled or clipped control may yield modal focus to the next
            // registered control, but its pending key press must not follow it.
            self.activate = false;
        }
        enabled && self.allows(scope) && *focus == Some(id)
    }

    pub fn activate(&mut self, focused: bool) -> bool {
        if focused && self.tab.is_none() && self.activate {
            self.activate = false;
            true
        } else {
            false
        }
    }

    /// Finalize the complete previous submission before starting a frame.
    /// Traversal uses current registrations, never a stale ordinal or widget.
    pub fn new_frame(&mut self, focus: &mut Option<Id>) {
        let closed = self.modal.is_some() && self.submitted_modal.is_none();
        if closed {
            self.modal = None;
            *focus = self.saved_focus.take();
            self.tab = None;
        }
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.scope == self.modal)
            .collect();
        let selected = entries.iter().position(|entry| Some(entry.id) == *focus);
        if let Some(backwards) = self.tab {
            *focus = if entries.is_empty() {
                None
            } else {
                let index = match (selected, backwards) {
                    (Some(index), true) => (index + entries.len() - 1) % entries.len(),
                    (Some(index), false) => (index + 1) % entries.len(),
                    (None, true) => entries.len() - 1,
                    (None, false) => 0,
                };
                Some(entries[index].id)
            };
        } else if selected.is_none()
            && (self.modal.is_some()
                || closed
                || focus.is_some_and(|id| self.previous_ids.contains(&id)))
        {
            *focus = if self.modal.is_some() {
                entries.first().map(|entry| entry.id)
            } else {
                None
            };
        }
        self.previous_ids.clear();
        self.previous_ids
            .extend(self.entries.iter().map(|entry| entry.id));
        self.entries.clear();
        self.submitted_modal = None;
        self.tab = None;
        self.activate = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_clipped_or_invalid_controls_are_not_keyboard_candidates() {
        use crate::math::Rect;
        let clip = Some(Rect::new(0., 0., 100., 100.));
        assert!(rect_visible(Rect::new(80., 80., 40., 30.), clip));
        assert!(!rect_visible(Rect::new(10., 100., 40., 30.), clip));
        assert!(!rect_visible(Rect::new(10., 10., 0., 30.), clip));
        assert!(!rect_visible(Rect::new(f32::NAN, 10., 10., 30.), None));
        assert!(!rect_visible(
            Rect::new(10., 10., 10., 30.),
            Some(Rect::default())
        ));
    }

    fn submit(state: &mut KeyboardFocus, focus: &mut Option<Id>, ids: &[Id]) {
        for &id in ids {
            state.register(id, None, true, focus);
        }
    }

    fn tab(state: &mut KeyboardFocus, focus: &mut Option<Id>, backwards: bool) {
        state.key_down(KeyCode::Tab, backwards, false);
        state.key_up(KeyCode::Tab);
        state.new_frame(focus);
    }

    #[test]
    fn traversal_starts_wraps_and_uses_stable_ids_after_reordering() {
        let mut state = KeyboardFocus::default();
        let mut focus = None;
        submit(&mut state, &mut focus, &[10, 20, 30]);
        tab(&mut state, &mut focus, false);
        assert_eq!(focus, Some(10));
        submit(&mut state, &mut focus, &[30, 10, 20]);
        tab(&mut state, &mut focus, false);
        assert_eq!(focus, Some(20));
        submit(&mut state, &mut focus, &[30, 10, 20]);
        tab(&mut state, &mut focus, false);
        assert_eq!(focus, Some(30));
        submit(&mut state, &mut focus, &[30, 10, 20]);
        tab(&mut state, &mut focus, true);
        assert_eq!(focus, Some(20));
    }

    #[test]
    fn removed_and_disabled_widgets_cannot_receive_focus_or_activate() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(20);
        submit(&mut state, &mut focus, &[10, 30]);
        tab(&mut state, &mut focus, false);
        assert_eq!(focus, Some(10));
        state.key_down(KeyCode::Enter, false, false);
        assert!(!state.register(10, None, false, &mut focus));
        assert_eq!(focus, None);
        assert!(!state.activate(false));
        submit(&mut state, &mut focus, &[30]);
        state.new_frame(&mut focus);
        assert_eq!(focus, None);
    }

    #[test]
    fn disabling_focused_modal_control_does_not_activate_its_replacement() {
        for key in [KeyCode::Enter, KeyCode::Space] {
            let mut state = KeyboardFocus::default();
            let mut focus = None;
            state.enter_modal(100, &mut focus);
            state.register(10, Some(100), true, &mut focus);
            state.register(20, Some(100), true, &mut focus);
            state.new_frame(&mut focus);
            state.enter_modal(100, &mut focus);
            state.key_down(key, false, false);
            assert!(!state.register(10, Some(100), false, &mut focus));
            assert!(state.register(20, Some(100), true, &mut focus));
            assert_eq!(focus, Some(20));
            assert!(
                !state.activate(true),
                "a press aimed at the disabled control must not activate its replacement"
            );
            state.key_up(key);
            state.key_down(key, false, false);
            assert!(
                state.activate(true),
                "a fresh press still activates the replacement"
            );
        }
    }

    #[test]
    fn activation_is_once_per_press_even_with_repeat_or_multiple_widgets() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(10);
        for key in [KeyCode::Enter, KeyCode::Space] {
            state.key_down(key, false, false);
            assert!(!state.activate(false));
            assert!(state.activate(true));
            assert!(!state.activate(true));
            submit(&mut state, &mut focus, &[10]);
            state.new_frame(&mut focus);
            state.key_down(key, false, false);
            assert!(!state.activate(true));
            state.key_up(key);
        }
        state.key_down(KeyCode::Enter, false, true);
        assert!(!state.activate(true));
    }

    #[test]
    fn modal_contains_focus_consumes_opener_and_restores_live_opener() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(10);
        state.key_down(KeyCode::Enter, false, false);
        state.enter_modal(100, &mut focus);
        assert!(state.register(20, Some(100), true, &mut focus));
        assert!(!state.activate(true));
        assert!(!state.register(10, None, true, &mut focus));
        state.register(30, Some(100), true, &mut focus);
        tab(&mut state, &mut focus, true);
        assert_eq!(focus, Some(30));
        // No popup submitted: restoration follows the complete closing frame.
        submit(&mut state, &mut focus, &[10]);
        state.new_frame(&mut focus);
        assert_eq!(focus, Some(10));
    }

    #[test]
    fn modal_does_not_restore_removed_opener_or_leak_to_background() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(10);
        state.enter_modal(100, &mut focus);
        state.register(20, Some(100), true, &mut focus);
        submit(&mut state, &mut focus, &[10]);
        state.new_frame(&mut focus);
        assert!(!state.register(30, None, true, &mut focus));
        state.new_frame(&mut focus);
        assert_eq!(focus, None);
    }

    #[test]
    fn empty_registry_clears_focus_and_tab_blocks_same_frame_activation() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(10);
        state.key_down(KeyCode::Enter, false, false);
        state.key_down(KeyCode::Tab, false, false);
        assert!(!state.activate(true));
        state.new_frame(&mut focus);
        assert_eq!(focus, None);
    }

    #[test]
    fn legacy_nontraversable_widget_focus_is_preserved_until_managed() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(50);
        submit(&mut state, &mut focus, &[10]);
        state.new_frame(&mut focus);
        assert_eq!(focus, Some(50));
        submit(&mut state, &mut focus, &[10, 50]);
        state.new_frame(&mut focus);
        submit(&mut state, &mut focus, &[10]);
        state.new_frame(&mut focus);
        assert_eq!(focus, None);
    }

    #[test]
    fn opening_keys_are_suppressed_until_release_without_blocking_other_keys() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(10);
        state.key_down(KeyCode::Space, false, false);
        state.enter_modal(100, &mut focus);
        assert!(state.suppress_key(KeyCode::Space));
        assert!(!state.suppress_key(KeyCode::Enter));
        state.key_down(KeyCode::Space, false, false);
        assert!(!state.activate(true));
        state.key_up(KeyCode::Space);
        assert!(!state.suppress_key(KeyCode::Space));
        state.key_down(KeyCode::Space, false, false);
        assert!(state.activate(true));
    }
}

#[cfg(test)]
mod checkbox_tests {
    use super::*;

    #[test]
    fn checkbox_space_and_enter_share_edges_and_do_not_double_toggle() {
        let mut state = KeyboardFocus::default();
        let mut focus = Some(2);
        // Editbox, checkbox, button in mixed submission order.
        for id in [1, 2, 3] {
            state.register(id, None, true, &mut focus);
        }
        state.new_frame(&mut focus);
        let mut checked = false;
        state.key_down(KeyCode::Space, false, false);
        state.key_down(KeyCode::Enter, false, false);
        if state.activate(true) {
            checked = !checked;
        }
        assert!(checked);
        assert!(!state.activate(true));
        for _ in 0..4 {
            state.register(2, None, true, &mut focus);
            state.new_frame(&mut focus);
            state.key_down(KeyCode::Space, false, false);
            state.key_down(KeyCode::Enter, false, false);
            assert!(!state.activate(true));
        }
        state.key_up(KeyCode::Enter);
        state.key_down(KeyCode::Enter, false, false);
        assert!(state.activate(true));
    }
}
