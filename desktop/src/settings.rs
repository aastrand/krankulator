use std::path::PathBuf;

use crate::bindings::{Action, GamepadButtonId, InputBindings, KeyId};

pub struct Settings {
    pub integer_scaling: bool,
    pub scanlines: bool,
    pub overscan: bool,
    pub bindings: InputBindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            integer_scaling: true,
            scanlines: false,
            overscan: true,
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
    settings
}

pub fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut content = format!(
        "integer_scaling={}\nscanlines={}\noverscan={}\n",
        settings.integer_scaling, settings.scanlines, settings.overscan
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
