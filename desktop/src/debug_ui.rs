use std::collections::HashMap;

use egui::epaint::PathStroke;
use krankulator_core::emu::debug::DebugSnapshot;
use pixels::Pixels;
use winit::window::Window;

pub const PANEL_WIDTH: f32 = 280.0;

pub struct DebugUi {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    sprite_textures: HashMap<u8, egui::TextureHandle>,
    nt_textures: Vec<egui::TextureHandle>,
}

impl DebugUi {
    pub fn new(window: &Window, pixels: &Pixels) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());

        let state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let device = pixels.device();
        let surface_format = pixels.surface_texture_format();
        let renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        Self {
            ctx,
            state,
            renderer,
            sprite_textures: HashMap::new(),
            nt_textures: Vec::new(),
        }
    }

    pub fn prepare_and_render(
        &mut self,
        window: &Window,
        snapshot: &DebugSnapshot,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        for sprite in &snapshot.sprites {
            let w = sprite.width as usize;
            let h = sprite.height as usize;
            let image = rgb_to_color_image(&sprite.pixels, w, h);
            if let Some(tex) = self.sprite_textures.get_mut(&sprite.index) {
                tex.set(image, egui::TextureOptions::NEAREST);
            } else {
                let tex = self.ctx.load_texture(
                    format!("spr_{}", sprite.index),
                    image,
                    egui::TextureOptions::NEAREST,
                );
                self.sprite_textures.insert(sprite.index, tex);
            }
        }
        let sprite_ids: Vec<u8> = snapshot.sprites.iter().map(|s| s.index).collect();
        self.sprite_textures.retain(|k, _| sprite_ids.contains(k));

        for (i, nt) in snapshot.nametables.iter().enumerate() {
            let w = nt.width as usize;
            let h = nt.height as usize;
            let image = rgb_to_color_image(&nt.pixels, w, h);
            if let Some(tex) = self.nt_textures.get_mut(i) {
                tex.set(image, egui::TextureOptions::NEAREST);
            } else {
                let tex =
                    self.ctx
                        .load_texture(format!("nt_{i}"), image, egui::TextureOptions::NEAREST);
                self.nt_textures.push(tex);
            }
        }

        let sprite_textures = &self.sprite_textures;
        let nt_textures = &self.nt_textures;
        let raw_input = self.state.take_egui_input(window);
        #[allow(deprecated)]
        let full_output = self.ctx.run(raw_input, |ctx| {
            build_ui(ctx, snapshot, sprite_textures, nt_textures);
        });

        self.state
            .handle_platform_output(window, full_output.platform_output);

        let primitives = self
            .ctx
            .tessellate(full_output.shapes, self.ctx.pixels_per_point());

        for (id, delta) in &full_output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }

        let size = window.inner_size();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point: self.ctx.pixels_per_point(),
        };

        self.renderer
            .update_buffers(device, queue, encoder, &primitives, &screen);

        let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: render_target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });
        self.renderer
            .render(&mut rpass.forget_lifetime(), &primitives, &screen);

        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}

fn rgb_to_color_image(rgb: &[u8], w: usize, h: usize) -> egui::ColorImage {
    let mut rgba = vec![255u8; w * h * 4];
    for i in 0..w * h {
        rgba[i * 4] = rgb[i * 3];
        rgba[i * 4 + 1] = rgb[i * 3 + 1];
        rgba[i * 4 + 2] = rgb[i * 3 + 2];
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

#[allow(deprecated)]
fn build_ui(
    ctx: &egui::Context,
    snapshot: &DebugSnapshot,
    sprite_textures: &HashMap<u8, egui::TextureHandle>,
    nt_textures: &[egui::TextureHandle],
) {
    egui::SidePanel::left("debug_left")
        .exact_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            draw_left_panel(ui, snapshot, nt_textures);
        });

    egui::SidePanel::right("debug_right")
        .exact_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            draw_right_panel(ui, snapshot, sprite_textures);
        });
}

fn draw_left_panel(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    nt_textures: &[egui::TextureHandle],
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading("Disassembly");
            ui.separator();

            ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

            for (i, line) in snapshot.disasm.iter().enumerate() {
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
                let text = format!("{:04X}: {} {}", line.addr, bytes_str, line.text);

                if is_pc {
                    let label = egui::RichText::new(text)
                        .color(egui::Color32::BLACK)
                        .background_color(egui::Color32::from_rgb(100, 200, 100));
                    ui.label(label);
                } else {
                    let label = egui::RichText::new(text).color(egui::Color32::LIGHT_GRAY);
                    ui.label(label);
                }
            }

            ui.add_space(8.0);
            draw_nametables(ui, nt_textures, &snapshot.ppu);
        });
}

fn draw_right_panel(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    sprite_textures: &HashMap<u8, egui::TextureHandle>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_cpu_registers(ui, snapshot);
            ui.add_space(8.0);
            draw_ppu_state(ui, snapshot);
            ui.add_space(8.0);
            draw_apu_waveforms(ui, snapshot);
            ui.add_space(8.0);
            draw_oam(ui, snapshot, sprite_textures);
        });
}

fn draw_cpu_registers(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("CPU");
    ui.separator();

    ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

    let cpu = &snapshot.cpu;
    ui.label(format!("PC: {:04X}  A: {:02X}", cpu.pc, cpu.a));
    ui.label(format!("X:  {:02X}    Y: {:02X}", cpu.x, cpu.y));
    ui.label(format!("SP: {:02X}    CYC: {}", cpu.sp, cpu.cycle));

    ui.add_space(4.0);

    let flags = [
        ('N', 0x80),
        ('V', 0x40),
        ('-', 0x20),
        ('B', 0x10),
        ('D', 0x08),
        ('I', 0x04),
        ('Z', 0x02),
        ('C', 0x01),
    ];

    ui.horizontal(|ui| {
        ui.label("Flags: ");
        for (name, bit) in &flags {
            let set = cpu.status & bit != 0;
            let color = if set {
                egui::Color32::from_rgb(100, 255, 100)
            } else {
                egui::Color32::from_rgb(80, 80, 80)
            };
            ui.label(egui::RichText::new(name.to_string()).color(color));
        }
    });
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
    let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 100, 100));

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

fn draw_apu_waveforms(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    ui.heading("APU");
    ui.separator();

    for ch in &snapshot.apu.channels {
        draw_waveform(ui, ch.name, &ch.waveform, ch.enabled, 40.0);
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
        painter.add(egui::Shape::line(points, PathStroke::new(1.0, wave_color)));
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
