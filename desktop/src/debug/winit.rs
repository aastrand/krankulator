use std::collections::HashMap;

use krankulator_core::emu::debug::DebugSnapshot;
use pixels::Pixels;
use winit::window::Window;

use super::common::{self, rgb_to_color_image};

pub struct DebugUi {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    sprite_textures: HashMap<u8, egui::TextureHandle>,
    nt_textures: Vec<egui::TextureHandle>,
    pt_textures: Vec<egui::TextureHandle>,
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
            pt_textures: Vec::new(),
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
        let raw_input = self.state.take_egui_input(window);
        #[allow(deprecated)]
        let full_output = self.ctx.run(raw_input, |ctx| {
            common::build_ui(ctx, snapshot, sprite_textures, nt_textures, pt_textures);
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
