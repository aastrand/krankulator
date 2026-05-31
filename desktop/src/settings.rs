use std::path::PathBuf;

pub struct Settings {
    pub integer_scaling: bool,
    pub scanlines: bool,
    pub overscan: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            integer_scaling: true,
            scanlines: false,
            overscan: true,
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
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "integer_scaling" => settings.integer_scaling = value.trim() == "true",
            "scanlines" => settings.scanlines = value.trim() == "true",
            "overscan" => settings.overscan = value.trim() == "true",
            _ => {}
        }
    }
    settings
}

pub fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let content = format!(
        "integer_scaling={}\nscanlines={}\noverscan={}\n",
        settings.integer_scaling, settings.scanlines, settings.overscan
    );
    let _ = std::fs::write(path, content);
}
