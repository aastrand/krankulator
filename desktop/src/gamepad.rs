use crate::bindings::{Action, InputBindings};

pub struct GamepadState {
    pub p1_bits: u8,
    pub p2_bits: u8,
    pub save_state: bool,
    pub load_state: bool,
    pub cycle_slot: bool,
    pub rewind: bool,
}

#[derive(Default)]
struct RawState {
    a: bool,
    b: bool,
    start: bool,
    select: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    left_shoulder: bool,
    right_shoulder: bool,
    left_trigger: bool,
    right_trigger: bool,
}

impl RawState {
    fn pressed_buttons(&self) -> impl Iterator<Item = &'static str> {
        [
            (self.a, "East"),
            (self.b, "South"),
            (self.start, "Start"),
            (self.select, "Select"),
            (self.up, "DPadUp"),
            (self.down, "DPadDown"),
            (self.left, "DPadLeft"),
            (self.right, "DPadRight"),
            (self.left_shoulder, "LeftTrigger"),
            (self.right_shoulder, "RightTrigger"),
            (self.left_trigger, "LeftTrigger2"),
            (self.right_trigger, "RightTrigger2"),
        ]
        .into_iter()
        .filter_map(|(pressed, name)| pressed.then_some(name))
    }
}

pub struct GamepadPollResult {
    pub states: [Option<GamepadState>; 2],
    pub toasts: Vec<String>,
}

struct MetaState {
    save: bool,
    load: bool,
    cycle: bool,
}

pub struct Gamepads {
    inner: PlatformGamepads,
    prev_meta: [MetaState; 2],
    was_connected: [bool; 2],
}

impl Gamepads {
    pub fn new() -> Self {
        Self {
            inner: PlatformGamepads::new(),
            prev_meta: [
                MetaState {
                    save: false,
                    load: false,
                    cycle: false,
                },
                MetaState {
                    save: false,
                    load: false,
                    cycle: false,
                },
            ],
            was_connected: [false; 2],
        }
    }

    pub fn poll_raw_buttons(&mut self) -> Vec<&'static str> {
        let (raw, _names) = self.inner.poll();
        let mut buttons = Vec::new();
        for state in &raw {
            if let Some(s) = state {
                buttons.extend(s.pressed_buttons());
            }
        }
        buttons
    }

    pub fn poll(&mut self, bindings: &InputBindings) -> GamepadPollResult {
        let (raw, _names) = self.inner.poll();
        let mut states: [Option<GamepadState>; 2] = [None, None];
        let mut toasts = Vec::new();

        for (i, state) in raw.iter().enumerate() {
            let is_connected = state.is_some();
            if is_connected && !self.was_connected[i] {
                toasts.push(format!("P{} CONNECTED", i + 1));
            } else if !is_connected && self.was_connected[i] {
                toasts.push(format!("P{} DISCONNECTED", i + 1));
            }
            self.was_connected[i] = is_connected;

            if let Some(s) = state {
                let mut p1_bits: u8 = 0;
                let mut p2_bits: u8 = 0;
                let mut save_raw = false;
                let mut load_raw = false;
                let mut cycle_raw = false;
                let mut rewind = false;

                for btn_name in s.pressed_buttons() {
                    for action in bindings.gamepad_action(btn_name) {
                        if let Some((player, bit)) = action.controller_bit() {
                            // Gamepad slot offsets the player: P1 actions on gamepad 1 go to P2
                            let effective = (player as usize + i) % 2;
                            if effective == 0 {
                                p1_bits |= bit;
                            } else {
                                p2_bits |= bit;
                            }
                        }
                        match action {
                            Action::SaveState => save_raw = true,
                            Action::LoadState => load_raw = true,
                            Action::CycleSlot => cycle_raw = true,
                            Action::Rewind => rewind = true,
                            _ => {}
                        }
                    }
                }

                let prev = &mut self.prev_meta[i];
                let save_edge = save_raw && !prev.save;
                let load_edge = load_raw && !prev.load;
                let cycle_edge = cycle_raw && !prev.cycle;
                prev.save = save_raw;
                prev.load = load_raw;
                prev.cycle = cycle_raw;

                states[i] = Some(GamepadState {
                    p1_bits,
                    p2_bits,
                    save_state: save_edge,
                    load_state: load_edge,
                    cycle_slot: cycle_edge,
                    rewind,
                });
            }
        }
        GamepadPollResult { states, toasts }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::RawState;
    use objc2_game_controller::{GCController, GCDevice};

    pub struct PlatformGamepads;

    impl PlatformGamepads {
        pub fn new() -> Self {
            Self
        }

        pub fn poll(&mut self) -> ([Option<RawState>; 2], [Option<String>; 2]) {
            let mut result: [Option<RawState>; 2] = [None, None];
            let mut names: [Option<String>; 2] = [None, None];
            let controllers = unsafe { GCController::controllers() };

            let mut slot = 0;
            for controller in controllers.iter() {
                if slot >= 2 {
                    break;
                }
                let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
                    continue;
                };

                let vendor = unsafe { controller.vendorName() }.map(|n| n.to_string());
                let is_joycon_pair = vendor.as_deref().is_some_and(|n| n.contains("Joy-Con"));

                if is_joycon_pair && slot == 0 {
                    names[0] = vendor.clone();
                    names[1] = vendor;
                    unsafe {
                        let rx = -gamepad.rightThumbstick().xAxis().value();
                        let ry = -gamepad.rightThumbstick().yAxis().value();
                        let sw_a = gamepad.buttonB().isPressed();
                        let sw_b = gamepad.buttonA().isPressed();
                        let sw_x = gamepad.buttonX().isPressed();
                        let sw_y = gamepad.buttonY().isPressed();
                        let r_sh = gamepad.rightShoulder().isPressed();
                        let r_tr = gamepad.rightTrigger().isPressed();

                        result[0] = Some(RawState {
                            up: rx > 0.5,
                            down: rx < -0.5,
                            left: ry > 0.5,
                            right: ry < -0.5,
                            a: sw_x,
                            b: sw_b,
                            start: gamepad.buttonMenu().isPressed(),
                            select: r_sh || r_tr,
                            left_shoulder: sw_a,
                            right_shoulder: sw_y,
                            left_trigger: false,
                            right_trigger: false,
                        });

                        let lx = gamepad.leftThumbstick().xAxis().value();
                        let ly = gamepad.leftThumbstick().yAxis().value();
                        let dpad = gamepad.dpad();
                        let l_sh = gamepad.leftShoulder().isPressed();
                        let l_tr = gamepad.leftTrigger().isPressed();

                        result[1] = Some(RawState {
                            up: lx > 0.5,
                            down: lx < -0.5,
                            left: ly > 0.5,
                            right: ly < -0.5,
                            a: dpad.down().isPressed(),
                            b: dpad.left().isPressed(),
                            start: gamepad.buttonOptions().is_some_and(|b| b.isPressed()),
                            select: l_sh || l_tr,
                            left_shoulder: false,
                            right_shoulder: false,
                            left_trigger: false,
                            right_trigger: false,
                        });
                    }
                    slot = 2;
                } else {
                    names[slot] = vendor;
                    unsafe {
                        let dpad = gamepad.dpad();
                        let lx = gamepad.leftThumbstick().xAxis().value();
                        let ly = gamepad.leftThumbstick().yAxis().value();
                        result[slot] = Some(RawState {
                            a: gamepad.buttonB().isPressed(),
                            b: gamepad.buttonA().isPressed(),
                            start: gamepad.buttonMenu().isPressed(),
                            select: gamepad.buttonOptions().is_some_and(|b| b.isPressed()),
                            up: dpad.up().isPressed() || ly > 0.5,
                            down: dpad.down().isPressed() || ly < -0.5,
                            left: dpad.left().isPressed() || lx < -0.5,
                            right: dpad.right().isPressed() || lx > 0.5,
                            left_shoulder: gamepad.leftShoulder().isPressed(),
                            right_shoulder: gamepad.rightShoulder().isPressed(),
                            left_trigger: gamepad.leftTrigger().isPressed(),
                            right_trigger: gamepad.rightTrigger().isPressed(),
                        });
                    }
                    slot += 1;
                }
            }
            (result, names)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::RawState;
    use gilrs::{Axis, Button, EventType, GamepadId, Gilrs, MappingSource};

    pub struct PlatformGamepads {
        gilrs: Gilrs,
        players: [Option<GamepadId>; 2],
        states: [RawState; 2],
    }

    impl PlatformGamepads {
        pub fn new() -> Self {
            let gilrs = Gilrs::new().unwrap();
            let mut players = [None; 2];
            for (id, gamepad) in gilrs.gamepads() {
                if gamepad.is_connected() && gamepad.mapping_source() == MappingSource::SdlMappings
                {
                    if players[0].is_none() {
                        players[0] = Some(id);
                    } else if players[1].is_none() {
                        players[1] = Some(id);
                    }
                }
            }
            Self {
                gilrs,
                players,
                states: [RawState::default(), RawState::default()],
            }
        }

        pub fn poll(&mut self) -> ([Option<RawState>; 2], [Option<String>; 2]) {
            while let Some(event) = self.gilrs.next_event() {
                let player = if self.players[0] == Some(event.id) {
                    0
                } else if self.players[1] == Some(event.id) {
                    1
                } else {
                    if matches!(event.event, EventType::Connected) {
                        let gp = self.gilrs.gamepad(event.id);
                        if gp.mapping_source() == MappingSource::SdlMappings {
                            if self.players[0].is_none() {
                                self.players[0] = Some(event.id);
                            } else if self.players[1].is_none() {
                                self.players[1] = Some(event.id);
                            }
                        }
                    }
                    continue;
                };

                match event.event {
                    EventType::ButtonPressed(btn, _) => {
                        self.apply_button(&btn, player, true);
                    }
                    EventType::ButtonReleased(btn, _) => {
                        self.apply_button(&btn, player, false);
                    }
                    EventType::AxisChanged(axis, value, _) => {
                        let s = &mut self.states[player];
                        match axis {
                            Axis::LeftStickX => {
                                s.left = value < -0.5;
                                s.right = value > 0.5;
                            }
                            Axis::LeftStickY => {
                                s.up = value > 0.5;
                                s.down = value < -0.5;
                            }
                            _ => {}
                        }
                    }
                    EventType::Disconnected => {
                        if self.players[0] == Some(event.id) {
                            self.players[0] = None;
                            self.states[0] = RawState::default();
                        } else if self.players[1] == Some(event.id) {
                            self.players[1] = None;
                            self.states[1] = RawState::default();
                        }
                    }
                    _ => {}
                }
            }

            let mut result: [Option<RawState>; 2] = [None, None];
            let mut names: [Option<String>; 2] = [None, None];
            for i in 0..2 {
                if let Some(id) = self.players[i] {
                    let s = &self.states[i];
                    result[i] = Some(RawState {
                        a: s.a,
                        b: s.b,
                        start: s.start,
                        select: s.select,
                        up: s.up,
                        down: s.down,
                        left: s.left,
                        right: s.right,
                        left_shoulder: s.left_shoulder,
                        right_shoulder: s.right_shoulder,
                        left_trigger: s.left_trigger,
                        right_trigger: s.right_trigger,
                    });
                    names[i] = Some(self.gilrs.gamepad(id).name().to_string());
                }
            }
            (result, names)
        }

        fn apply_button(&mut self, btn: &Button, player: usize, pressed: bool) {
            let s = &mut self.states[player];
            match btn {
                Button::East => s.a = pressed,
                Button::South => s.b = pressed,
                Button::Start => s.start = pressed,
                Button::Select => s.select = pressed,
                Button::DPadUp => s.up = pressed,
                Button::DPadDown => s.down = pressed,
                Button::DPadLeft => s.left = pressed,
                Button::DPadRight => s.right = pressed,
                Button::LeftTrigger => s.left_shoulder = pressed,
                Button::RightTrigger => s.right_shoulder = pressed,
                Button::LeftTrigger2 => s.left_trigger = pressed,
                Button::RightTrigger2 => s.right_trigger = pressed,
                _ => {}
            }
        }
    }
}

use platform::PlatformGamepads;
