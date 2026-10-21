#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyCode {
    Up,
    Down,
    Right,
    Left,
    Backspace,
    Delete,
    Enter,
    Tab,
    Space,
    Home,
    End,
    Control,
    Escape,
    A, // select all
    Z, // undo
    Y, // redo
    C, // copy
    V, // paste
    X, // cut
}

pub trait InputHandler {
    fn mouse_down(&mut self, position: (f32, f32));
    fn mouse_up(&mut self, _: (f32, f32));
    fn mouse_wheel(&mut self, x: f32, y: f32);
    fn mouse_move(&mut self, position: (f32, f32));
    fn char_event(&mut self, character: char, shift: bool, ctrl: bool);
    /// Report a press or held key. Keyboard button activation ignores repeats.
    fn key_down(&mut self, key_down: KeyCode, shift: bool, ctrl: bool);
    /// Report release to rearm Tab and button activation. The default keeps
    /// existing external input handlers source compatible.
    fn key_up(&mut self, _key: KeyCode) {}
}
