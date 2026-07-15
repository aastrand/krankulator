use std::collections::HashMap;

use egui::epaint::PathStroke;
use krankulator_core::emu::debug::DebugSnapshot;
use krankulator_core::emu::gfx::palette;

pub const PANEL_WIDTH: f32 = 280.0;

pub fn rgb_to_color_image(rgb: &[u8], w: usize, h: usize) -> egui::ColorImage {
    let mut rgba = vec![255u8; w * h * 4];
    for i in 0..w * h {
        rgba[i * 4] = rgb[i * 3];
        rgba[i * 4 + 1] = rgb[i * 3 + 1];
        rgba[i * 4 + 2] = rgb[i * 3 + 2];
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

#[allow(deprecated)]
pub fn build_ui(
    ctx: &egui::Context,
    snapshot: &DebugSnapshot,
    sprite_textures: &HashMap<u8, egui::TextureHandle>,
    nt_textures: &[egui::TextureHandle],
    pt_textures: &[egui::TextureHandle],
) {
    egui::SidePanel::left("debug_left")
        .exact_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            draw_left_panel(ui, snapshot, nt_textures, pt_textures, sprite_textures);
        });

    egui::SidePanel::right("debug_right")
        .exact_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            draw_right_panel(ui, snapshot);
        });
}

fn draw_left_panel(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    nt_textures: &[egui::TextureHandle],
    pt_textures: &[egui::TextureHandle],
    sprite_textures: &HashMap<u8, egui::TextureHandle>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_ppu_state(ui, snapshot);
            ui.add_space(8.0);
            draw_palette(ui, &snapshot.palette);
            ui.add_space(8.0);
            draw_pattern_tables(ui, pt_textures);
            ui.add_space(8.0);
            draw_nametables(ui, nt_textures, &snapshot.ppu);
            ui.add_space(8.0);
            draw_oam(ui, snapshot, sprite_textures);
        });
}

fn draw_right_panel(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_disassembly(ui, snapshot);
            ui.add_space(8.0);
            draw_cpu_and_stack(ui, snapshot);
            ui.add_space(8.0);
            draw_apu_waveforms(ui, snapshot);
        });
}

fn draw_disassembly(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("Disassembly");
    ui.separator();

    ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

    use krankulator_core::emu::debug::DISASM_CONTEXT;
    let disasm_lines = DISASM_CONTEXT * 2 + 1;

    for i in 0..disasm_lines {
        if let Some(line) = snapshot.disasm.get(i) {
            let is_pc = i == snapshot.disasm_pc_index;
            let bytes_str = match line.byte_count {
                1 => format!("{:02X}      ", line.bytes[0]),
                2 => format!("{:02X} {:02X}   ", line.bytes[0], line.bytes[1]),
                3 => format!(
                    "{:02X} {:02X} {:02X}",
                    line.bytes[0], line.bytes[1], line.bytes[2]
                ),
                _ => "         ".to_string(),
            };
            let mut text = format!("{:04X}: {} {}", line.addr, bytes_str, line.text);
            if let Some(detail) = &line.operand_detail {
                text.push_str(&format!("  {detail}"));
            }

            let label = if is_pc {
                egui::RichText::new(text)
                    .color(egui::Color32::BLACK)
                    .background_color(egui::Color32::from_rgb(100, 200, 100))
            } else {
                egui::RichText::new(text).color(egui::Color32::LIGHT_GRAY)
            };
            ui.add(egui::Label::new(label).wrap_mode(egui::TextWrapMode::Truncate));
        } else {
            ui.label(" ");
        }
    }
}

fn draw_cpu_and_stack(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("CPU");
    ui.separator();

    ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

    let cpu = &snapshot.cpu;
    ui.label(format!(
        "PC:{:04X} A:{:02X} X:{:02X} Y:{:02X} SP:{:02X}",
        cpu.pc, cpu.a, cpu.x, cpu.y, cpu.sp
    ));
    ui.horizontal(|ui| {
        ui.label(format!("CYC:{}", cpu.cycle));
        for &(name, bit) in &[
            ('N', 0x80),
            ('V', 0x40),
            ('-', 0x20),
            ('B', 0x10),
            ('D', 0x08),
            ('I', 0x04),
            ('Z', 0x02),
            ('C', 0x01),
        ] {
            let set = cpu.status & bit != 0;
            let color = if set {
                egui::Color32::from_rgb(100, 255, 100)
            } else {
                egui::Color32::from_rgb(80, 80, 80)
            };
            ui.label(egui::RichText::new(name.to_string()).color(color));
        }
    });

    if !snapshot.stack.is_empty() {
        let sp = cpu.sp;
        let addr = 0x0100u16 + sp.wrapping_add(1) as u16;
        let bytes: Vec<String> = snapshot
            .stack
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();
        ui.label(format!("Stack ({:04X}): {}", addr, bytes.join(" ")));
    }
}

fn draw_ppu_state(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("PPU");
    ui.separator();

    ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

    let ppu = &snapshot.ppu;
    ui.label(format!(
        "CTRL: {:02X}  MASK: {:02X}  STAT: {:02X}",
        ppu.ctrl, ppu.mask, ppu.status
    ));
    ui.label(format!("V: {:04X}  T: {:04X}", ppu.v, ppu.t));
    ui.label(format!(
        "Scroll: ({},{})  Frame: {}",
        ppu.scroll_x, ppu.scroll_y, ppu.frame
    ));
}

fn draw_palette(ui: &mut egui::Ui, palette_ram: &[u8; 32]) {
    ui.heading("Palette");
    ui.separator();

    let avail_w = ui.available_width();
    let gap = 6.0;
    let cell_spacing = 1.0;
    let label_w = 20.0;
    let cells_w = avail_w - label_w - gap * 3.0 - cell_spacing * 12.0;
    let cell_size = (cells_w / 16.0).floor().max(4.0);

    let sections = [("BG", 0usize), ("SP", 16usize)];
    for (label, base) in &sections {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(
                egui::RichText::new(*label)
                    .monospace()
                    .size(9.0)
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);
            for pal in 0..4 {
                if pal > 0 {
                    ui.add_space(gap);
                }
                for col in 0..4 {
                    let nes_idx = if col == 0 {
                        palette_ram[0] as usize % palette::PALETTE_SIZE
                    } else {
                        palette_ram[base + pal * 4 + col] as usize % palette::PALETTE_SIZE
                    };
                    let (r, g, b) = palette::PALETTE[nes_idx];
                    let color = egui::Color32::from_rgb(r, g, b);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(cell_size, cell_size),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 0.0, color);
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(0.5_f32, egui::Color32::from_rgb(60, 60, 60)),
                        egui::StrokeKind::Outside,
                    );
                    if col < 3 {
                        ui.add_space(cell_spacing);
                    }
                }
            }
        });
    }
}

fn draw_nametables(
    ui: &mut egui::Ui,
    nt_textures: &[egui::TextureHandle],
    ppu: &krankulator_core::emu::debug::PpuSnapshot,
) {
    ui.heading("Nametables");
    ui.separator();

    let labels = ["$2000", "$2400", "$2800", "$2C00"];
    let avail_w = ui.available_width();
    let spacing = 2.0;
    let cell_w = (avail_w - spacing) / 2.0;
    let aspect = 256.0 / 240.0;
    let cell_h = cell_w / aspect;

    for row in nt_textures.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for tex in row {
                let i = nt_textures
                    .iter()
                    .position(|t| t.id() == tex.id())
                    .unwrap_or(0);
                let size = egui::vec2(cell_w, cell_h);

                ui.vertical(|ui| {
                    if i < labels.len() {
                        ui.label(
                            egui::RichText::new(labels[i])
                                .color(egui::Color32::GRAY)
                                .monospace()
                                .size(9.0),
                        );
                    }
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    draw_scroll_viewport(ui, rect, i, cell_w, cell_h, ppu);
                });
            }
        });
        ui.add_space(2.0);
    }
}

fn draw_scroll_viewport(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    nt_index: usize,
    img_w: f32,
    img_h: f32,
    ppu: &krankulator_core::emu::debug::PpuSnapshot,
) {
    let nt_x_bit = ppu.nametable_select & 1;
    let nt_y_bit = (ppu.nametable_select >> 1) & 1;
    let scroll_origin_x = nt_x_bit as f32 * 256.0 + ppu.scroll_x as f32;
    let scroll_origin_y = nt_y_bit as f32 * 240.0 + ppu.scroll_y as f32;
    let nt_origin_x = (nt_index % 2) as f32 * 256.0;
    let nt_origin_y = (nt_index / 2) as f32 * 240.0;

    let scale_x = img_w / 256.0;
    let scale_y = img_h / 240.0;
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 100, 100));

    for dx in [0.0, -512.0, 512.0] {
        let vp_x = scroll_origin_x + dx - nt_origin_x;

        if vp_x >= 256.0 || vp_x + 256.0 <= 0.0 {
            continue;
        }

        let x0 = (vp_x.max(0.0) * scale_x) + rect.left();
        let x1 = ((vp_x + 256.0).min(256.0) * scale_x) + rect.left();

        for dy in [0.0, -480.0, 480.0] {
            let vp_y = scroll_origin_y + dy - nt_origin_y;

            if vp_y >= 240.0 || vp_y + 240.0 <= 0.0 {
                continue;
            }

            let y0 = (vp_y.max(0.0) * scale_y) + rect.top();
            let y1 = ((vp_y + 240.0).min(240.0) * scale_y) + rect.top();

            let vp_rect = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
            ui.painter()
                .rect_stroke(vp_rect, 0.0, stroke, egui::StrokeKind::Outside);
        }
    }
}

fn draw_pattern_tables(ui: &mut egui::Ui, pt_textures: &[egui::TextureHandle]) {
    ui.heading("Pattern Tables");
    ui.separator();

    if pt_textures.len() < 2 {
        return;
    }

    let labels = ["$0000", "$1000"];
    let avail_w = ui.available_width();
    let spacing = 4.0;
    let cell_w = (avail_w - spacing) / 2.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        for (i, tex) in pt_textures.iter().enumerate() {
            let size = egui::vec2(cell_w, cell_w);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(labels[i])
                        .color(egui::Color32::GRAY)
                        .monospace()
                        .size(9.0),
                );
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            });
        }
    });
}

fn draw_apu_waveforms(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("APU");
    ui.separator();

    for ch in &snapshot.apu.channels {
        draw_waveform(ui, ch.name, &ch.waveform, ch.enabled, 40.0);
    }
    if !snapshot.apu.expansion_channels.is_empty() {
        ui.separator();
        for ch in &snapshot.apu.expansion_channels {
            draw_waveform(ui, ch.name, &ch.waveform, ch.enabled, 40.0);
        }
    }
    draw_waveform(ui, "Mix", &snapshot.apu.mixed_waveform, true, 50.0);
}

fn draw_waveform(ui: &mut egui::Ui, label: &str, waveform: &[f32], enabled: bool, height: f32) {
    let label_color = if enabled {
        egui::Color32::from_rgb(100, 200, 100)
    } else {
        egui::Color32::from_rgb(120, 120, 120)
    };
    ui.label(
        egui::RichText::new(label)
            .color(label_color)
            .monospace()
            .size(10.0),
    );

    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let rect = response.rect;

    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 30));

    if waveform.is_empty() {
        return;
    }

    let wave_color = if enabled {
        egui::Color32::from_rgb(80, 200, 80)
    } else {
        egui::Color32::from_rgb(60, 60, 60)
    };

    let max_val = waveform.iter().fold(0.01_f32, |acc, &v| acc.max(v.abs()));

    let step = waveform.len() as f32 / rect.width();
    let points: Vec<egui::Pos2> = (0..rect.width() as usize)
        .map(|x| {
            let idx = (x as f32 * step) as usize;
            let idx = idx.min(waveform.len() - 1);
            let val = waveform[idx] / max_val;
            let y = rect.center().y - val * (rect.height() * 0.45);
            egui::pos2(rect.left() + x as f32, y)
        })
        .collect();

    if points.len() >= 2 {
        painter.add(egui::Shape::line(
            points,
            PathStroke::new(1.0_f32, wave_color),
        ));
    }
}

fn draw_oam(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    sprite_textures: &HashMap<u8, egui::TextureHandle>,
) {
    ui.heading(format!("Sprites ({})", snapshot.sprites.len()));
    ui.separator();

    let scale = 2.0;
    let spacing = 2.0;
    let avail_w = ui.available_width();

    let textures: Vec<_> = snapshot
        .sprites
        .iter()
        .filter_map(|s| {
            sprite_textures
                .get(&s.index)
                .map(|tex| (s.width as f32 * scale, s.height as f32 * scale, tex))
        })
        .collect();

    if textures.is_empty() {
        return;
    }

    let item_w = textures[0].0 + spacing;
    let cols = ((avail_w + spacing) / item_w).floor().max(1.0) as usize;

    for row in textures.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for &(w, h, tex) in row {
                ui.image(egui::load::SizedTexture::new(tex.id(), egui::vec2(w, h)));
            }
        });
    }
}
