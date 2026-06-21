use krankulator_core::emu::io::controller;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    P1A,
    P1B,
    P1Start,
    P1Select,
    P1Up,
    P1Down,
    P1Left,
    P1Right,
    P2A,
    P2B,
    P2Start,
    P2Select,
    P2Up,
    P2Down,
    P2Left,
    P2Right,
    SaveState,
    LoadState,
    CycleSlot,
    Reset,
    Rewind,
    FastForward,
    Mute,
    ToggleOverlay,
    ToggleScaling,
    ToggleScanlines,
    ToggleDebug,
    Pause,
    Fullscreen,
}

impl Action {
    pub fn to_settings_key(self) -> &'static str {
        match self {
            Action::P1A => "p1_a",
            Action::P1B => "p1_b",
            Action::P1Start => "p1_start",
            Action::P1Select => "p1_select",
            Action::P1Up => "p1_up",
            Action::P1Down => "p1_down",
            Action::P1Left => "p1_left",
            Action::P1Right => "p1_right",
            Action::P2A => "p2_a",
            Action::P2B => "p2_b",
            Action::P2Start => "p2_start",
            Action::P2Select => "p2_select",
            Action::P2Up => "p2_up",
            Action::P2Down => "p2_down",
            Action::P2Left => "p2_left",
            Action::P2Right => "p2_right",
            Action::SaveState => "save_state",
            Action::LoadState => "load_state",
            Action::CycleSlot => "cycle_slot",
            Action::Reset => "reset",
            Action::Rewind => "rewind",
            Action::FastForward => "fast_forward",
            Action::Mute => "mute",
            Action::ToggleOverlay => "toggle_overlay",
            Action::ToggleScaling => "toggle_scaling",
            Action::ToggleScanlines => "toggle_scanlines",
            Action::ToggleDebug => "toggle_debug",
            Action::Pause => "pause",
            Action::Fullscreen => "fullscreen",
        }
    }

    pub fn from_settings_key(key: &str) -> Option<Action> {
        match key {
            "p1_a" => Some(Action::P1A),
            "p1_b" => Some(Action::P1B),
            "p1_start" => Some(Action::P1Start),
            "p1_select" => Some(Action::P1Select),
            "p1_up" => Some(Action::P1Up),
            "p1_down" => Some(Action::P1Down),
            "p1_left" => Some(Action::P1Left),
            "p1_right" => Some(Action::P1Right),
            "p2_a" => Some(Action::P2A),
            "p2_b" => Some(Action::P2B),
            "p2_start" => Some(Action::P2Start),
            "p2_select" => Some(Action::P2Select),
            "p2_up" => Some(Action::P2Up),
            "p2_down" => Some(Action::P2Down),
            "p2_left" => Some(Action::P2Left),
            "p2_right" => Some(Action::P2Right),
            "save_state" => Some(Action::SaveState),
            "load_state" => Some(Action::LoadState),
            "cycle_slot" => Some(Action::CycleSlot),
            "reset" => Some(Action::Reset),
            "rewind" => Some(Action::Rewind),
            "fast_forward" => Some(Action::FastForward),
            "mute" => Some(Action::Mute),
            "toggle_overlay" => Some(Action::ToggleOverlay),
            "toggle_scaling" => Some(Action::ToggleScaling),
            "toggle_scanlines" => Some(Action::ToggleScanlines),
            "toggle_debug" => Some(Action::ToggleDebug),
            "pause" => Some(Action::Pause),
            "fullscreen" => Some(Action::Fullscreen),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Action::P1A => "P1 A",
            Action::P1B => "P1 B",
            Action::P1Start => "P1 Start",
            Action::P1Select => "P1 Select",
            Action::P1Up => "P1 Up",
            Action::P1Down => "P1 Down",
            Action::P1Left => "P1 Left",
            Action::P1Right => "P1 Right",
            Action::P2A => "P2 A",
            Action::P2B => "P2 B",
            Action::P2Start => "P2 Start",
            Action::P2Select => "P2 Select",
            Action::P2Up => "P2 Up",
            Action::P2Down => "P2 Down",
            Action::P2Left => "P2 Left",
            Action::P2Right => "P2 Right",
            Action::SaveState => "Save State",
            Action::LoadState => "Load State",
            Action::CycleSlot => "Cycle Slot",
            Action::Reset => "Reset",
            Action::Rewind => "Rewind",
            Action::FastForward => "Fast Forward",
            Action::Mute => "Mute",
            Action::ToggleOverlay => "Overlay",
            Action::ToggleScaling => "Scaling",
            Action::ToggleScanlines => "CRT Scanlines",
            Action::ToggleDebug => "Debug View",
            Action::Pause => "Pause",
            Action::Fullscreen => "Fullscreen",
        }
    }

    pub fn controller_bit(self) -> Option<(u8, u8)> {
        match self {
            Action::P1A => Some((0, controller::A)),
            Action::P1B => Some((0, controller::B)),
            Action::P1Start => Some((0, controller::START)),
            Action::P1Select => Some((0, controller::SELECT)),
            Action::P1Up => Some((0, controller::UP)),
            Action::P1Down => Some((0, controller::DOWN)),
            Action::P1Left => Some((0, controller::LEFT)),
            Action::P1Right => Some((0, controller::RIGHT)),
            Action::P2A => Some((1, controller::A)),
            Action::P2B => Some((1, controller::B)),
            Action::P2Start => Some((1, controller::START)),
            Action::P2Select => Some((1, controller::SELECT)),
            Action::P2Up => Some((1, controller::UP)),
            Action::P2Down => Some((1, controller::DOWN)),
            Action::P2Left => Some((1, controller::LEFT)),
            Action::P2Right => Some((1, controller::RIGHT)),
            _ => None,
        }
    }

    pub const ALL: &'static [Action] = &[
        Action::P1A,
        Action::P1B,
        Action::P1Start,
        Action::P1Select,
        Action::P1Up,
        Action::P1Down,
        Action::P1Left,
        Action::P1Right,
        Action::P2A,
        Action::P2B,
        Action::P2Start,
        Action::P2Select,
        Action::P2Up,
        Action::P2Down,
        Action::P2Left,
        Action::P2Right,
        Action::SaveState,
        Action::LoadState,
        Action::CycleSlot,
        Action::Reset,
        Action::Rewind,
        Action::FastForward,
        Action::Mute,
        Action::ToggleOverlay,
        Action::ToggleScaling,
        Action::ToggleScanlines,
        Action::ToggleDebug,
        Action::Pause,
        Action::Fullscreen,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId(pub String);

impl KeyId {
    pub fn display_name(&self) -> &str {
        let s = self.0.as_str();
        match s {
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "Space" => "Space",
            "Tab" => "Tab",
            _ => {
                if let Some(rest) = s.strip_prefix("Key") {
                    rest
                } else if let Some(rest) = s.strip_prefix("Digit") {
                    rest
                } else {
                    s
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn from_winit(key: winit::keyboard::KeyCode) -> Self {
        KeyId(format!("{key:?}"))
    }

    #[cfg(target_os = "linux")]
    pub fn from_gdk(key: gdk::keys::Key) -> Option<Self> {
        use gdk::keys::constants as k;
        let name = match key {
            v if v == k::z || v == k::Z => "KeyZ",
            v if v == k::x || v == k::X => "KeyX",
            v if v == k::c || v == k::C => "KeyC",
            v if v == k::v || v == k::V => "KeyV",
            v if v == k::a || v == k::A => "KeyA",
            v if v == k::b || v == k::B => "KeyB",
            v if v == k::d || v == k::D => "KeyD",
            v if v == k::e || v == k::E => "KeyE",
            v if v == k::f || v == k::F => "KeyF",
            v if v == k::g || v == k::G => "KeyG",
            v if v == k::h || v == k::H => "KeyH",
            v if v == k::i || v == k::I => "KeyI",
            v if v == k::j || v == k::J => "KeyJ",
            v if v == k::k || v == k::K => "KeyK",
            v if v == k::l || v == k::L => "KeyL",
            v if v == k::m || v == k::M => "KeyM",
            v if v == k::n || v == k::N => "KeyN",
            v if v == k::o || v == k::O => "KeyO",
            v if v == k::p || v == k::P => "KeyP",
            v if v == k::q || v == k::Q => "KeyQ",
            v if v == k::r || v == k::R => "KeyR",
            v if v == k::s || v == k::S => "KeyS",
            v if v == k::t || v == k::T => "KeyT",
            v if v == k::u || v == k::U => "KeyU",
            v if v == k::w || v == k::W => "KeyW",
            v if v == k::y || v == k::Y => "KeyY",
            v if v == k::Left => "ArrowLeft",
            v if v == k::Right => "ArrowRight",
            v if v == k::Up => "ArrowUp",
            v if v == k::Down => "ArrowDown",
            v if v == k::space => "Space",
            v if v == k::Tab => "Tab",
            v if v == k::Return => "Enter",
            v if v == k::Escape => "Escape",
            v if v == k::F1 => "F1",
            v if v == k::F2 => "F2",
            v if v == k::F3 => "F3",
            v if v == k::F4 => "F4",
            v if v == k::F5 => "F5",
            v if v == k::F6 => "F6",
            v if v == k::F7 => "F7",
            v if v == k::F8 => "F8",
            v if v == k::F9 => "F9",
            v if v == k::F10 => "F10",
            v if v == k::F11 => "F11",
            v if v == k::F12 => "F12",
            v if v == k::_0 => "Digit0",
            v if v == k::_1 => "Digit1",
            v if v == k::_2 => "Digit2",
            v if v == k::_3 => "Digit3",
            v if v == k::_4 => "Digit4",
            v if v == k::_5 => "Digit5",
            v if v == k::_6 => "Digit6",
            v if v == k::_7 => "Digit7",
            v if v == k::_8 => "Digit8",
            v if v == k::_9 => "Digit9",
            v if v == k::comma => "Comma",
            v if v == k::period => "Period",
            v if v == k::slash => "Slash",
            v if v == k::semicolon => "Semicolon",
            v if v == k::apostrophe => "Quote",
            v if v == k::bracketleft => "BracketLeft",
            v if v == k::bracketright => "BracketRight",
            v if v == k::backslash => "Backslash",
            v if v == k::minus => "Minus",
            v if v == k::equal => "Equal",
            v if v == k::BackSpace => "Backspace",
            v if v == k::Shift_L || v == k::Shift_R => "Shift",
            v if v == k::Control_L || v == k::Control_R => "Control",
            v if v == k::Alt_L || v == k::Alt_R => "Alt",
            _ => return None,
        };
        Some(KeyId(name.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GamepadButtonId(pub String);

impl GamepadButtonId {
    pub fn display_name(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone)]
pub struct InputBindings {
    pub keyboard: Vec<(KeyId, Action)>,
    pub gamepad: Vec<(GamepadButtonId, Action)>,
}

impl Default for InputBindings {
    fn default() -> Self {
        Self {
            keyboard: vec![
                (KeyId("KeyZ".into()), Action::P1A),
                (KeyId("KeyX".into()), Action::P1B),
                (KeyId("KeyC".into()), Action::P1Start),
                (KeyId("KeyV".into()), Action::P1Select),
                (KeyId("ArrowUp".into()), Action::P1Up),
                (KeyId("ArrowDown".into()), Action::P1Down),
                (KeyId("ArrowLeft".into()), Action::P1Left),
                (KeyId("ArrowRight".into()), Action::P1Right),
                (KeyId("KeyS".into()), Action::SaveState),
                (KeyId("KeyA".into()), Action::LoadState),
                (KeyId("KeyQ".into()), Action::CycleSlot),
                (KeyId("KeyR".into()), Action::Reset),
                (KeyId("KeyW".into()), Action::Rewind),
                (KeyId("Space".into()), Action::FastForward),
                (KeyId("KeyM".into()), Action::Mute),
                (KeyId("Tab".into()), Action::ToggleOverlay),
                (KeyId("KeyI".into()), Action::ToggleScaling),
                (KeyId("F9".into()), Action::ToggleScanlines),
                (KeyId("F12".into()), Action::ToggleDebug),
                (KeyId("KeyP".into()), Action::Pause),
                (KeyId("F11".into()), Action::Fullscreen),
            ],
            gamepad: vec![
                (GamepadButtonId("East".into()), Action::P1A),
                (GamepadButtonId("South".into()), Action::P1B),
                (GamepadButtonId("Start".into()), Action::P1Start),
                (GamepadButtonId("Select".into()), Action::P1Select),
                (GamepadButtonId("DPadUp".into()), Action::P1Up),
                (GamepadButtonId("DPadDown".into()), Action::P1Down),
                (GamepadButtonId("DPadLeft".into()), Action::P1Left),
                (GamepadButtonId("DPadRight".into()), Action::P1Right),
                (GamepadButtonId("RightTrigger".into()), Action::SaveState),
                (GamepadButtonId("LeftTrigger".into()), Action::LoadState),
                (GamepadButtonId("LeftTrigger2".into()), Action::CycleSlot),
                (GamepadButtonId("RightTrigger2".into()), Action::Rewind),
            ],
        }
    }
}

impl InputBindings {
    pub fn keyboard_action<'a>(&'a self, key: &'a KeyId) -> impl Iterator<Item = Action> + 'a {
        self.keyboard
            .iter()
            .filter(move |(k, _)| k == key)
            .map(|(_, a)| *a)
    }

    pub fn gamepad_action<'a>(&'a self, button: &'a str) -> impl Iterator<Item = Action> + 'a {
        self.gamepad
            .iter()
            .filter(move |(b, _)| b.0 == button)
            .map(|(_, a)| *a)
    }

    pub fn keyboard_binding_for(&self, action: Action) -> Option<&KeyId> {
        self.keyboard
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(k, _)| k)
    }

    pub fn gamepad_binding_for(&self, action: Action) -> Option<&GamepadButtonId> {
        self.gamepad
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(b, _)| b)
    }

    pub fn set_keyboard_binding(&mut self, action: Action, key: KeyId) {
        self.keyboard.retain(|(_, a)| *a != action);
        self.keyboard.push((key, action));
    }

    pub fn set_gamepad_binding(&mut self, action: Action, button: GamepadButtonId) {
        self.gamepad.retain(|(_, a)| *a != action);
        self.gamepad.push((button, action));
    }

    pub fn clear_keyboard_binding(&mut self, action: Action) {
        self.keyboard.retain(|(_, a)| *a != action);
    }

    pub fn clear_gamepad_binding(&mut self, action: Action) {
        self.gamepad.retain(|(_, a)| *a != action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings_contain_all_nes_buttons() {
        let b = InputBindings::default();
        assert!(b.keyboard_binding_for(Action::P1A).is_some());
        assert!(b.keyboard_binding_for(Action::P1B).is_some());
        assert!(b.keyboard_binding_for(Action::P1Start).is_some());
        assert!(b.keyboard_binding_for(Action::P1Select).is_some());
        assert!(b.keyboard_binding_for(Action::P1Up).is_some());
        assert!(b.keyboard_binding_for(Action::P1Down).is_some());
        assert!(b.keyboard_binding_for(Action::P1Left).is_some());
        assert!(b.keyboard_binding_for(Action::P1Right).is_some());
    }

    #[test]
    fn test_keyboard_action_lookup() {
        let b = InputBindings::default();
        let actions: Vec<_> = b.keyboard_action(&KeyId("KeyZ".into())).collect();
        assert_eq!(actions, vec![Action::P1A]);
    }

    #[test]
    fn test_set_keyboard_binding_replaces() {
        let mut b = InputBindings::default();
        b.set_keyboard_binding(Action::P1A, KeyId("KeyN".into()));
        assert_eq!(
            b.keyboard_binding_for(Action::P1A),
            Some(&KeyId("KeyN".into()))
        );
        let z_actions: Vec<_> = b.keyboard_action(&KeyId("KeyZ".into())).collect();
        assert!(z_actions.is_empty());
    }

    #[test]
    fn test_clear_keyboard_binding() {
        let mut b = InputBindings::default();
        b.clear_keyboard_binding(Action::P1A);
        assert!(b.keyboard_binding_for(Action::P1A).is_none());
    }

    #[test]
    fn test_settings_key_roundtrip() {
        for action in Action::ALL {
            let key = action.to_settings_key();
            assert_eq!(Action::from_settings_key(key), Some(*action));
        }
    }

    #[test]
    fn test_controller_bit() {
        assert_eq!(Action::P1A.controller_bit(), Some((0, controller::A)));
        assert_eq!(Action::P2Left.controller_bit(), Some((1, controller::LEFT)));
        assert_eq!(Action::SaveState.controller_bit(), None);
    }

    #[test]
    fn test_key_display_name() {
        assert_eq!(KeyId("KeyZ".into()).display_name(), "Z");
        assert_eq!(KeyId("ArrowUp".into()).display_name(), "Up");
        assert_eq!(KeyId("F11".into()).display_name(), "F11");
        assert_eq!(KeyId("Space".into()).display_name(), "Space");
        assert_eq!(KeyId("Digit1".into()).display_name(), "1");
    }
}
