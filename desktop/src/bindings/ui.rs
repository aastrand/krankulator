use krankulator_core::emu::gfx::{buf::Buffer, font::draw_string};

use super::{Action, GamepadButtonId, InputBindings, KeyId};

const WHITE: (u8, u8, u8) = (255, 255, 255);
const BLACK: (u8, u8, u8) = (0, 0, 0);
const YELLOW: (u8, u8, u8) = (255, 255, 0);
const GRAY: (u8, u8, u8) = (160, 160, 160);
const DIM: (u8, u8, u8) = (100, 100, 100);

const ITEMS_PER_PAGE: usize = 20;

const BINDABLE_ACTIONS: &[Action] = Action::ALL;

fn p2_to_p1(action: Action) -> Option<Action> {
    match action {
        Action::P2A => Some(Action::P1A),
        Action::P2B => Some(Action::P1B),
        Action::P2Start => Some(Action::P1Start),
        Action::P2Select => Some(Action::P1Select),
        Action::P2Up => Some(Action::P1Up),
        Action::P2Down => Some(Action::P1Down),
        Action::P2Left => Some(Action::P1Left),
        Action::P2Right => Some(Action::P1Right),
        _ => None,
    }
}

fn effective_gamepad_binding(bindings: &InputBindings, action: Action) -> Option<String> {
    if let Some(b) = bindings.gamepad_binding_for(action) {
        return Some(b.display_name().to_string());
    }
    if let Some(p1) = p2_to_p1(action) {
        if let Some(b) = bindings.gamepad_binding_for(p1) {
            return Some(format!("{}(G2)", b.display_name()));
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum WaitKind {
    Keyboard,
    Gamepad,
}

enum State {
    SelectAction { cursor: usize, scroll: usize },
    ActionMenu { action_idx: usize, cursor: usize },
    WaitingForInput { action_idx: usize, kind: WaitKind },
}

pub enum UiEvent {
    None,
    Close,
    BindingsChanged,
}

pub struct BindingUi {
    state: State,
    active: bool,
}

impl BindingUi {
    pub fn new() -> Self {
        Self {
            state: State::SelectAction {
                cursor: 0,
                scroll: 0,
            },
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.state = State::SelectAction {
            cursor: 0,
            scroll: 0,
        };
    }

    pub fn handle_key(&mut self, key: &KeyId, bindings: &mut InputBindings) -> UiEvent {
        if !self.active {
            return UiEvent::None;
        }

        match &self.state {
            State::SelectAction { cursor, scroll } => {
                let cursor = *cursor;
                let scroll = *scroll;
                match key.0.as_str() {
                    "Escape" => {
                        self.active = false;
                        return UiEvent::Close;
                    }
                    "ArrowUp" | "KeyW" if cursor > 0 => {
                        let new_cursor = cursor - 1;
                        let new_scroll = if new_cursor < scroll {
                            new_cursor
                        } else {
                            scroll
                        };
                        self.state = State::SelectAction {
                            cursor: new_cursor,
                            scroll: new_scroll,
                        };
                    }
                    "ArrowDown" | "KeyS" if cursor + 1 < BINDABLE_ACTIONS.len() => {
                        let new_cursor = cursor + 1;
                        let new_scroll = if new_cursor >= scroll + ITEMS_PER_PAGE {
                            new_cursor - ITEMS_PER_PAGE + 1
                        } else {
                            scroll
                        };
                        self.state = State::SelectAction {
                            cursor: new_cursor,
                            scroll: new_scroll,
                        };
                    }
                    "Enter" | "KeyZ" => {
                        self.state = State::ActionMenu {
                            action_idx: cursor,
                            cursor: 0,
                        };
                    }
                    _ => {}
                }
            }
            State::ActionMenu {
                action_idx,
                cursor: menu_cursor,
            } => {
                let action_idx = *action_idx;
                let menu_cursor = *menu_cursor;
                match key.0.as_str() {
                    "Escape" | "KeyX" => {
                        let scroll = if action_idx >= ITEMS_PER_PAGE {
                            action_idx - ITEMS_PER_PAGE + 1
                        } else {
                            0
                        };
                        self.state = State::SelectAction {
                            cursor: action_idx,
                            scroll,
                        };
                    }
                    "ArrowUp" | "KeyW" if menu_cursor > 0 => {
                        self.state = State::ActionMenu {
                            action_idx,
                            cursor: menu_cursor - 1,
                        };
                    }
                    "ArrowDown" if menu_cursor < 3 => {
                        self.state = State::ActionMenu {
                            action_idx,
                            cursor: menu_cursor + 1,
                        };
                    }
                    "Enter" | "KeyZ" => match menu_cursor {
                        0 => {
                            self.state = State::WaitingForInput {
                                action_idx,
                                kind: WaitKind::Keyboard,
                            };
                        }
                        1 => {
                            self.state = State::WaitingForInput {
                                action_idx,
                                kind: WaitKind::Gamepad,
                            };
                        }
                        2 => {
                            let action = BINDABLE_ACTIONS[action_idx];
                            let defaults = InputBindings::default();
                            if let Some(dk) = defaults.keyboard_binding_for(action) {
                                bindings.set_keyboard_binding(action, dk.clone());
                            } else {
                                bindings.clear_keyboard_binding(action);
                            }
                            if let Some(db) = defaults.gamepad_binding_for(action) {
                                bindings.set_gamepad_binding(action, db.clone());
                            } else {
                                bindings.clear_gamepad_binding(action);
                            }
                            let scroll = if action_idx >= ITEMS_PER_PAGE {
                                action_idx - ITEMS_PER_PAGE + 1
                            } else {
                                0
                            };
                            self.state = State::SelectAction {
                                cursor: action_idx,
                                scroll,
                            };
                            return UiEvent::BindingsChanged;
                        }
                        3 => {
                            let scroll = if action_idx >= ITEMS_PER_PAGE {
                                action_idx - ITEMS_PER_PAGE + 1
                            } else {
                                0
                            };
                            self.state = State::SelectAction {
                                cursor: action_idx,
                                scroll,
                            };
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            State::WaitingForInput { action_idx, kind } => {
                if *kind == WaitKind::Keyboard {
                    let action_idx = *action_idx;
                    if key.0 == "Escape" {
                        self.state = State::ActionMenu {
                            action_idx,
                            cursor: 0,
                        };
                    } else {
                        let action = BINDABLE_ACTIONS[action_idx];
                        bindings.set_keyboard_binding(action, key.clone());
                        let scroll = if action_idx >= ITEMS_PER_PAGE {
                            action_idx - ITEMS_PER_PAGE + 1
                        } else {
                            0
                        };
                        self.state = State::SelectAction {
                            cursor: action_idx,
                            scroll,
                        };
                        return UiEvent::BindingsChanged;
                    }
                }
            }
        }
        UiEvent::None
    }

    pub fn handle_gamepad_button(&mut self, button: &str, bindings: &mut InputBindings) -> UiEvent {
        if !self.active {
            return UiEvent::None;
        }

        if let State::WaitingForInput { action_idx, kind } = &self.state {
            if *kind == WaitKind::Gamepad {
                let action_idx = *action_idx;
                let action = BINDABLE_ACTIONS[action_idx];
                bindings.set_gamepad_binding(action, GamepadButtonId(button.to_string()));
                let scroll = if action_idx >= ITEMS_PER_PAGE {
                    action_idx - ITEMS_PER_PAGE + 1
                } else {
                    0
                };
                self.state = State::SelectAction {
                    cursor: action_idx,
                    scroll,
                };
                return UiEvent::BindingsChanged;
            }
        }
        UiEvent::None
    }

    pub fn draw(&self, buf: &mut Buffer, bindings: &InputBindings) {
        if !self.active {
            return;
        }

        dim_background(buf);

        match &self.state {
            State::SelectAction { cursor, scroll } => {
                draw_string(buf, 8, 4, "INPUT SETTINGS", YELLOW, BLACK);
                draw_string(buf, 8, 16, "ESC:Back  ENTER:Edit", DIM, BLACK);

                let visible_end = (*scroll + ITEMS_PER_PAGE).min(BINDABLE_ACTIONS.len());
                for (vi, i) in (*scroll..visible_end).enumerate() {
                    let y = 30 + vi as i32 * 10;
                    let action = BINDABLE_ACTIONS[i];
                    let selected = i == *cursor;

                    let name = action.display_name();
                    let kb = bindings
                        .keyboard_binding_for(action)
                        .map(|k| k.display_name().to_string())
                        .unwrap_or_else(|| "-".into());
                    let gp =
                        effective_gamepad_binding(bindings, action).unwrap_or_else(|| "-".into());

                    let fg = if selected { YELLOW } else { WHITE };
                    let prefix = if selected { ">" } else { " " };

                    draw_string(buf, 4, y, prefix, YELLOW, BLACK);
                    draw_string(buf, 12, y, name, fg, BLACK);

                    let binding_str = format!("{}/{}", truncate(&kb, 6), truncate(&gp, 6));
                    let bx = 256 - (binding_str.len() as i32) * 8 - 4;
                    draw_string(buf, bx, y, &binding_str, GRAY, BLACK);
                }

                if *scroll > 0 {
                    draw_string(buf, 120, 26, "^", DIM, BLACK);
                }
                if visible_end < BINDABLE_ACTIONS.len() {
                    let bottom_y = 30 + ITEMS_PER_PAGE as i32 * 10;
                    draw_string(buf, 120, bottom_y, "v", DIM, BLACK);
                }
            }
            State::ActionMenu {
                action_idx,
                cursor: menu_cursor,
            } => {
                let action = BINDABLE_ACTIONS[*action_idx];
                draw_string(buf, 8, 4, action.display_name(), YELLOW, BLACK);

                let options = ["Set Key...", "Set Button...", "Restore Default", "Back"];
                for (i, label) in options.iter().enumerate() {
                    let y = 30 + i as i32 * 14;
                    let selected = i == *menu_cursor;
                    let fg = if selected { YELLOW } else { WHITE };
                    let prefix = if selected { ">" } else { " " };
                    draw_string(buf, 20, y, prefix, YELLOW, BLACK);
                    draw_string(buf, 28, y, label, fg, BLACK);
                }

                let kb = bindings
                    .keyboard_binding_for(action)
                    .map(|k| format!("Key: {}", k.display_name()))
                    .unwrap_or_else(|| "Key: -".into());
                let gp = effective_gamepad_binding(bindings, action)
                    .map(|s| format!("Btn: {s}"))
                    .unwrap_or_else(|| "Btn: -".into());
                draw_string(buf, 20, 100, &kb, GRAY, BLACK);
                draw_string(buf, 20, 114, &gp, GRAY, BLACK);
            }
            State::WaitingForInput { action_idx, kind } => {
                let action = BINDABLE_ACTIONS[*action_idx];
                let prompt = match kind {
                    WaitKind::Keyboard => format!("PRESS KEY FOR {}...", action.display_name()),
                    WaitKind::Gamepad => format!("PRESS BUTTON FOR {}...", action.display_name()),
                };
                draw_string(buf, 8, 110, &prompt, YELLOW, BLACK);
                draw_string(buf, 8, 126, "ESC to cancel", DIM, BLACK);
            }
        }
    }
}

fn dim_background(buf: &mut Buffer) {
    for byte in buf.data.iter_mut() {
        *byte >>= 2;
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}~", &s[..max - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_and_close() {
        let mut ui = BindingUi::new();
        assert!(!ui.is_active());
        ui.open();
        assert!(ui.is_active());
        let mut bindings = InputBindings::default();
        let event = ui.handle_key(&KeyId("Escape".into()), &mut bindings);
        assert!(matches!(event, UiEvent::Close));
        assert!(!ui.is_active());
    }

    #[test]
    fn test_navigate_and_rebind_key() {
        let mut ui = BindingUi::new();
        let mut bindings = InputBindings::default();
        ui.open();

        ui.handle_key(&KeyId("Enter".into()), &mut bindings);

        ui.handle_key(&KeyId("Enter".into()), &mut bindings);

        let event = ui.handle_key(&KeyId("KeyN".into()), &mut bindings);
        assert!(matches!(event, UiEvent::BindingsChanged));
        assert_eq!(
            bindings.keyboard_binding_for(Action::P1A),
            Some(&KeyId("KeyN".into()))
        );
    }

    #[test]
    fn test_rebind_gamepad() {
        let mut ui = BindingUi::new();
        let mut bindings = InputBindings::default();
        ui.open();

        ui.handle_key(&KeyId("Enter".into()), &mut bindings);

        ui.handle_key(&KeyId("ArrowDown".into()), &mut bindings);
        ui.handle_key(&KeyId("Enter".into()), &mut bindings);

        let event = ui.handle_gamepad_button("North", &mut bindings);
        assert!(matches!(event, UiEvent::BindingsChanged));
        assert_eq!(
            bindings.gamepad_binding_for(Action::P1A),
            Some(&GamepadButtonId("North".into()))
        );
    }

    #[test]
    fn test_restore_default() {
        let mut ui = BindingUi::new();
        let mut bindings = InputBindings::default();
        bindings.set_keyboard_binding(Action::P1A, KeyId("KeyN".into()));
        ui.open();

        ui.handle_key(&KeyId("Enter".into()), &mut bindings);

        ui.handle_key(&KeyId("ArrowDown".into()), &mut bindings);
        ui.handle_key(&KeyId("ArrowDown".into()), &mut bindings);
        let event = ui.handle_key(&KeyId("Enter".into()), &mut bindings);
        assert!(matches!(event, UiEvent::BindingsChanged));
        assert_eq!(
            bindings.keyboard_binding_for(Action::P1A),
            Some(&KeyId("KeyZ".into()))
        );
    }

    #[test]
    fn test_draw_does_not_panic() {
        let mut ui = BindingUi::new();
        let bindings = InputBindings::default();
        let mut buf = Buffer::new();
        ui.open();
        ui.draw(&mut buf, &bindings);
    }

    #[test]
    fn test_escape_from_waiting() {
        let mut ui = BindingUi::new();
        let mut bindings = InputBindings::default();
        ui.open();
        ui.handle_key(&KeyId("Enter".into()), &mut bindings);
        ui.handle_key(&KeyId("Enter".into()), &mut bindings);
        let event = ui.handle_key(&KeyId("Escape".into()), &mut bindings);
        assert!(matches!(event, UiEvent::None));
        assert!(ui.is_active());
        assert_eq!(
            bindings.keyboard_binding_for(Action::P1A),
            Some(&KeyId("KeyZ".into()))
        );
    }
}
