use super::buf::Buffer;
use super::font;

const TOAST_DURATION: u32 = 120;
const TOAST_LINE_SPACING: i32 = 10;
const NTSC_FRAME_MS: f64 = 16.64;
const FG: (u8, u8, u8) = (255, 255, 255);
const OUTLINE: (u8, u8, u8) = (0, 0, 0);

struct Toast {
    text: String,
    frames_remaining: u32,
}

pub struct Overlay {
    enabled: bool,
    frame_time_text: String,
    toasts: Vec<Toast>,
    banner: Option<String>,
    rewind_status: Option<String>,
    overscan: u8,
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            enabled: false,
            frame_time_text: String::new(),
            toasts: Vec::new(),
            banner: None,
            rewind_status: None,
            overscan: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn set_frame_time(&mut self, emu_ms: f64) {
        let budget_pct = (emu_ms / NTSC_FRAME_MS) * 100.0;
        self.frame_time_text = format!("{:.1}ms ({:.0}%)", emu_ms, budget_pct);
    }

    pub fn set_banner(&mut self, text: Option<String>) {
        self.banner = text;
    }

    pub fn set_rewind_status(&mut self, text: Option<String>) {
        self.rewind_status = text;
    }

    pub fn set_overscan(&mut self, lines: u8) {
        self.overscan = lines;
    }

    pub fn toast(&mut self, text: String) {
        if self.toasts.len() >= 4 {
            self.toasts.remove(0);
        }
        self.toasts.push(Toast {
            text,
            frames_remaining: TOAST_DURATION,
        });
    }

    pub fn tick(&mut self) {
        self.toasts.retain_mut(|t| {
            t.frames_remaining = t.frames_remaining.saturating_sub(1);
            t.frames_remaining > 0
        });
    }

    pub fn draw(&self, buf: &mut Buffer) {
        let os = self.overscan as i32;
        if self.enabled && !self.frame_time_text.is_empty() {
            font::draw_string(buf, 2, 2 + os, &self.frame_time_text, FG, OUTLINE);
        }

        if let Some(ref banner) = self.banner {
            let x = (buf.width as i32 - banner.len() as i32 * 8) / 2;
            let y = buf.height as i32 / 2 - 4;
            font::draw_string(buf, x, y, banner, FG, OUTLINE);
        }

        let mut y = buf.height as i32 - 12 - os;
        if let Some(ref text) = self.rewind_status {
            let x = (buf.width as i32 - text.len() as i32 * 8) / 2;
            font::draw_string(buf, x, y, text, FG, OUTLINE);
            y -= TOAST_LINE_SPACING;
        }
        for toast in self.toasts.iter().rev() {
            let x = (buf.width as i32 - toast.text.len() as i32 * 8) / 2;
            font::draw_string(buf, x, y, &toast.text, FG, OUTLINE);
            y -= TOAST_LINE_SPACING;
        }
    }
}
