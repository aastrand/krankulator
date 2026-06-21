use std::path::PathBuf;

use crate::bindings::{Action, GamepadButtonId, InputBindings, KeyId};

pub struct Settings {
    pub integer_scaling: bool,
    pub scanlines: bool,
    pub overscan: bool,
    pub correct_aspect_ratio: bool,
    pub window_scale: u32,
    pub bindings: InputBindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            integer_scaling: true,
            scanlines: false,
            overscan: true,
            correct_aspect_ratio: true,
            window_scale: 4,
            bindings: InputBindings::default(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    crate::config_dir().map(|d| d.join("settings.txt"))
}

pub fn load_settings() -> Settings {
    let mut settings = Settings::default();
    let Some(path) = settings_path() else {
        return settings;
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return settings;
    };
    let mut has_bindings = false;
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "integer_scaling" => settings.integer_scaling = value == "true",
            "scanlines" => settings.scanlines = value == "true",
            "overscan" => settings.overscan = value == "true",
            "correct_aspect_ratio" => settings.correct_aspect_ratio = value == "true",
            "window_scale" => {
                if let Ok(s) = value.parse::<u32>() {
                    settings.window_scale = s.clamp(1, 6);
                }
            }
            _ => {
                if let Some(rest) = key.strip_prefix("bind_kb_") {
                    if let Some(action) = Action::from_settings_key(rest) {
                        if !has_bindings {
                            settings.bindings.keyboard.clear();
                            settings.bindings.gamepad.clear();
                            has_bindings = true;
                        }
                        settings
                            .bindings
                            .keyboard
                            .push((KeyId(value.to_string()), action));
                    }
                } else if let Some(rest) = key.strip_prefix("bind_gp_") {
                    if let Some(action) = Action::from_settings_key(rest) {
                        if !has_bindings {
                            settings.bindings.keyboard.clear();
                            settings.bindings.gamepad.clear();
                            has_bindings = true;
                        }
                        settings
                            .bindings
                            .gamepad
                            .push((GamepadButtonId(value.to_string()), action));
                    }
                }
            }
        }
    }
    if has_bindings {
        let defaults = InputBindings::default();
        let loaded_kb_actions: std::collections::HashSet<_> =
            settings.bindings.keyboard.iter().map(|(_, a)| *a).collect();
        for (key, action) in &defaults.keyboard {
            if !loaded_kb_actions.contains(action) {
                settings.bindings.keyboard.push((key.clone(), *action));
            }
        }
        let loaded_gp_actions: std::collections::HashSet<_> =
            settings.bindings.gamepad.iter().map(|(_, a)| *a).collect();
        for (btn, action) in &defaults.gamepad {
            if !loaded_gp_actions.contains(action) {
                settings.bindings.gamepad.push((btn.clone(), *action));
            }
        }
    }
    settings
}

pub fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut content =
        format!(
        "integer_scaling={}\nscanlines={}\noverscan={}\ncorrect_aspect_ratio={}\nwindow_scale={}\n",
        settings.integer_scaling, settings.scanlines, settings.overscan,
        settings.correct_aspect_ratio, settings.window_scale
    );
    for (key, action) in &settings.bindings.keyboard {
        content.push_str(&format!("bind_kb_{}={}\n", action.to_settings_key(), key.0));
    }
    for (button, action) in &settings.bindings.gamepad {
        content.push_str(&format!(
            "bind_gp_{}={}\n",
            action.to_settings_key(),
            button.0
        ));
    }
    let _ = std::fs::write(path, content);
}
