pub struct GamepadState {
    pub a: bool,
    pub b: bool,
    pub start: bool,
    pub select: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub save_state: bool,
    pub load_state: bool,
    pub cycle_slot: bool,
}

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
}

impl Default for RawState {
    fn default() -> Self {
        Self {
            a: false,
            b: false,
            start: false,
            select: false,
            up: false,
            down: false,
            left: false,
            right: false,
            left_shoulder: false,
            right_shoulder: false,
            left_trigger: false,
        }
    }
}

pub struct Gamepads {
    inner: PlatformGamepads,
    prev_shoulders: [[bool; 3]; 2],
}

impl Gamepads {
    pub fn new() -> Self {
        Self {
            inner: PlatformGamepads::new(),
            prev_shoulders: [[false; 3]; 2],
        }
    }

    pub fn poll(&mut self) -> [Option<GamepadState>; 2] {
        let raw = self.inner.poll();
        let mut result: [Option<GamepadState>; 2] = [None, None];
        for (i, state) in raw.iter().enumerate() {
            if let Some(s) = state {
                let prev = &mut self.prev_shoulders[i];
                let save = s.right_shoulder && !prev[0];
                let load = s.left_shoulder && !prev[1];
                let cycle = s.left_trigger && !prev[2];
                prev[0] = s.right_shoulder;
                prev[1] = s.left_shoulder;
                prev[2] = s.left_trigger;
                result[i] = Some(GamepadState {
                    a: s.a,
                    b: s.b,
                    start: s.start,
                    select: s.select,
                    up: s.up,
                    down: s.down,
                    left: s.left,
                    right: s.right,
                    save_state: save,
                    load_state: load,
                    cycle_slot: cycle,
                });
            }
        }
        result
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

        pub fn poll(&mut self) -> [Option<RawState>; 2] {
            let mut result: [Option<RawState>; 2] = [None, None];
            let controllers = unsafe { GCController::controllers() };

            let mut slot = 0;
            for controller in controllers.iter() {
                if slot >= 2 {
                    break;
                }
                let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
                    continue;
                };

                let is_joycon_pair = unsafe { controller.vendorName() }
                    .map_or(false, |n| n.to_string().contains("Joy-Con"));

                if is_joycon_pair && slot == 0 {
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
                            start: gamepad.buttonOptions().map_or(false, |b| b.isPressed()),
                            select: l_sh || l_tr,
                            left_shoulder: false,
                            right_shoulder: false,
                            left_trigger: false,
                        });
                    }
                    slot = 2;
                } else {
                    unsafe {
                        let dpad = gamepad.dpad();
                        let lx = gamepad.leftThumbstick().xAxis().value();
                        let ly = gamepad.leftThumbstick().yAxis().value();
                        result[slot] = Some(RawState {
                            a: gamepad.buttonB().isPressed(),
                            b: gamepad.buttonA().isPressed(),
                            start: gamepad.buttonMenu().isPressed(),
                            select: gamepad.buttonOptions().map_or(false, |b| b.isPressed()),
                            up: dpad.up().isPressed() || ly > 0.5,
                            down: dpad.down().isPressed() || ly < -0.5,
                            left: dpad.left().isPressed() || lx < -0.5,
                            right: dpad.right().isPressed() || lx > 0.5,
                            left_shoulder: gamepad.leftShoulder().isPressed(),
                            right_shoulder: gamepad.rightShoulder().isPressed(),
                            left_trigger: gamepad.leftTrigger().isPressed(),
                        });
                    }
                    slot += 1;
                }
            }
            result
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::RawState;
    use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

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
                if gamepad.is_connected() {
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

        pub fn poll(&mut self) -> [Option<RawState>; 2] {
            while let Some(event) = self.gilrs.next_event() {
                let player = if self.players[0] == Some(event.id) {
                    0
                } else if self.players[1] == Some(event.id) {
                    1
                } else {
                    if matches!(event.event, EventType::Connected) {
                        if self.players[0].is_none() {
                            self.players[0] = Some(event.id);
                        } else if self.players[1].is_none() {
                            self.players[1] = Some(event.id);
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
                        } else if self.players[1] == Some(event.id) {
                            self.players[1] = None;
                        }
                    }
                    _ => {}
                }
            }

            let mut result: [Option<RawState>; 2] = [None, None];
            for i in 0..2 {
                if self.players[i].is_some() {
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
                    });
                }
            }
            result
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
                _ => {}
            }
        }
    }
}

use platform::PlatformGamepads;
