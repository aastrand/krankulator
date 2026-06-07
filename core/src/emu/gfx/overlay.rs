use super::buf::Buffer;
use super::font;

const TOAST_DURATION: u32 = 120;
const TOAST_LINE_SPACING: i32 = 10;
const FG: (u8, u8, u8) = (255, 255, 255);
const OUTLINE: (u8, u8, u8) = (0, 0, 0);

struct Toast {
    text: String,
    frames_remaining: u32,
}

enum TransportStatus {
    Rewind(String),
    FastForward,
}

pub struct Overlay {
    enabled: bool,
    frame_time_text: String,
    frame_budget_ms: f64,
    toasts: Vec<Toast>,
    banner: Option<String>,
    transport: Option<TransportStatus>,
    overscan: u8,
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            enabled: false,
            frame_time_text: String::new(),
            frame_budget_ms: 16.64,
            toasts: Vec::new(),
            banner: None,
            transport: None,
            overscan: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn set_frame_budget_ms(&mut self, ms: f64) {
        self.frame_budget_ms = ms;
    }

    pub fn set_frame_time(&mut self, emu_ms: f64) {
        let budget_pct = (emu_ms / self.frame_budget_ms) * 100.0;
        self.frame_time_text = format!("{emu_ms:.1}ms ({budget_pct:.0}%)");
    }

    pub fn set_banner(&mut self, text: Option<String>) {
        self.banner = text;
    }

    pub fn set_rewind_status(&mut self, text: Option<String>) {
        self.transport = text.map(TransportStatus::Rewind);
    }

    pub fn set_fast_forward(&mut self, active: bool) {
        match (&self.transport, active) {
            (Some(TransportStatus::Rewind(_)), _) => {}
            (_, true) => self.transport = Some(TransportStatus::FastForward),
            (_, false) => self.transport = None,
        }
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
        match &self.transport {
            Some(TransportStatus::Rewind(time_text)) => {
                let label = "REWIND ";
                let arrow_w = 10;
                let space_w = 4;
                let time_w = time_text.len() as i32 * 8;
                let total_w = label.len() as i32 * 8 + arrow_w + space_w + time_w;
                let x = (buf.width as i32 - total_w) / 2;
                font::draw_string(buf, x, y, label, FG, OUTLINE);
                let ax = x + label.len() as i32 * 8;
                font::draw_double_arrow(buf, ax, y, font::ArrowDir::Left, FG, OUTLINE);
                font::draw_string(buf, ax + arrow_w + space_w, y, time_text, FG, OUTLINE);
                y -= TOAST_LINE_SPACING;
            }
            Some(TransportStatus::FastForward) => {
                let label = "FF ";
                let arrow_w = 10;
                let total_w = label.len() as i32 * 8 + arrow_w;
                let x = (buf.width as i32 - total_w) / 2;
                font::draw_string(buf, x, y, label, FG, OUTLINE);
                font::draw_double_arrow(
                    buf,
                    x + label.len() as i32 * 8,
                    y,
                    font::ArrowDir::Right,
                    FG,
                    OUTLINE,
                );
                y -= TOAST_LINE_SPACING;
            }
            None => {}
        }
        for toast in self.toasts.iter().rev() {
            let x = (buf.width as i32 - toast.text.len() as i32 * 8) / 2;
            font::draw_string(buf, x, y, &toast.text, FG, OUTLINE);
            y -= TOAST_LINE_SPACING;
        }
    }
}
