use std::collections::HashMap;
use std::sync::Arc;

use glow::HasContext;
use krankulator_core::emu::debug::DebugSnapshot;

use super::common::{self, rgb_to_color_image};

pub struct DebugUi {
    ctx: egui::Context,
    painter: egui_glow::Painter,
    sprite_textures: HashMap<u8, egui::TextureHandle>,
    nt_textures: Vec<egui::TextureHandle>,
    pt_textures: Vec<egui::TextureHandle>,
}

impl DebugUi {
    pub fn new(gl: &Arc<glow::Context>) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());

        let painter = egui_glow::Painter::new(gl.clone(), "", None, false)
            .expect("Failed to create egui_glow painter");

        Self {
            ctx,
            painter,
            sprite_textures: HashMap::new(),
            nt_textures: Vec::new(),
            pt_textures: Vec::new(),
        }
    }

    pub fn render(
        &mut self,
        gl: &glow::Context,
        snapshot: &DebugSnapshot,
        width_pixels: u32,
        height_pixels: u32,
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

        for (i, pt) in snapshot.pattern_tables.iter().enumerate() {
            let w = pt.width as usize;
            let h = pt.height as usize;
            let image = rgb_to_color_image(&pt.pixels, w, h);
            if let Some(tex) = self.pt_textures.get_mut(i) {
                tex.set(image, egui::TextureOptions::NEAREST);
            } else {
                let tex =
                    self.ctx
                        .load_texture(format!("pt_{i}"), image, egui::TextureOptions::NEAREST);
                self.pt_textures.push(tex);
            }
        }

        let sprite_textures = &self.sprite_textures;
        let nt_textures = &self.nt_textures;
        let pt_textures = &self.pt_textures;

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(width_pixels as f32, height_pixels as f32),
            )),
            ..Default::default()
        };

        #[allow(deprecated)]
        let full_output = self.ctx.run(raw_input, |ctx| {
            common::build_ui(ctx, snapshot, sprite_textures, nt_textures, pt_textures);
        });

        let primitives = self
            .ctx
            .tessellate(full_output.shapes, self.ctx.pixels_per_point());

        self.painter.paint_and_update_textures(
            [width_pixels, height_pixels],
            self.ctx.pixels_per_point(),
            &primitives,
            &full_output.textures_delta,
        );

        // Restore GL state for next frame's NES rendering
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.use_program(None);
            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn destroy(&mut self) {
        self.painter.destroy();
    }
}
