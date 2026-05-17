use super::buf::Buffer;
use super::font;

const TOAST_DURATION: u32 = 120;
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
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            enabled: false,
            frame_time_text: String::new(),
            toasts: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn set_frame_time(&mut self, emu_ms: f64) {
        let budget_pct = (emu_ms / 16.64) * 100.0;
        self.frame_time_text = format!("{:.1}ms ({:.0}%)", emu_ms, budget_pct);
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
        if self.enabled && !self.frame_time_text.is_empty() {
            font::draw_string(buf, 2, 2, &self.frame_time_text, FG, OUTLINE);
        }

        let mut y = buf.height as i32 - 12;
        for toast in self.toasts.iter().rev() {
            let x = (buf.width as i32 - toast.text.len() as i32 * 8) / 2;
            font::draw_string(buf, x, y, &toast.text, FG, OUTLINE);
            y -= 10;
        }
    }
}
