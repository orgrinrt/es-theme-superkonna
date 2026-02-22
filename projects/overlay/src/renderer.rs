//! Renderer using tiny-skia.
//! Draws themed achievement popups and the in-game menu overlay.

use crate::config::MenuConfig;
use crate::menu::{Menu, MenuState};
use crate::theme::Theme;
use tiny_skia::*;

pub struct Renderer {
    fg: Color8,
    bg: Color8,
    accent: Color8,
    on_accent: Color8,
    card: Color8,
    display_font: fontdue::Font,
    body_font: fontdue::Font,
    light_font: fontdue::Font,
}

#[derive(Clone, Copy)]
struct Color8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Color8 {
    fn from_theme(c: &crate::theme::Color) -> Self {
        Color8 { r: c.r, g: c.g, b: c.b, a: c.a }
    }

    fn with_alpha(self, a: u8) -> Self {
        Color8 { a, ..self }
    }
}

impl Renderer {
    pub fn new(theme: &Theme) -> Self {
        Renderer {
            fg: Color8::from_theme(&theme.fg_color),
            bg: Color8::from_theme(&theme.bg_color),
            accent: Color8::from_theme(&theme.accent_color),
            on_accent: Color8::from_theme(&theme.on_accent_color),
            card: Color8::from_theme(&theme.card_color),
            display_font: load_font(&theme.font_display_path),
            body_font: load_font(&theme.font_path),
            light_font: load_font(&theme.font_light_path),
        }
    }

    // ── Popup rendering (existing) ──────────────────────────

    pub fn render_popup(&self, title: &str, description: &str, opacity: f32) -> Vec<u32> {
        let w: u32 = 480;
        let h: u32 = 120;
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let opacity_byte = (opacity * 255.0) as u8;

        // Drop shadow (subtle offset)
        draw_rounded_rect(&mut pixmap, 4.0, 4.0, w as f32 - 4.0, h as f32 - 4.0, 14.0,
            Color8 { r: 0, g: 0, b: 0, a: (60.0 * opacity) as u8 });

        // Background card
        draw_rounded_rect(&mut pixmap, 2.0, 2.0, w as f32 - 6.0, h as f32 - 6.0, 12.0,
            self.card.with_alpha((self.card.a as f32 * opacity) as u8));

        // Accent left strip
        draw_rounded_rect(&mut pixmap, 2.0, 2.0, 6.0, h as f32 - 6.0, 3.0,
            self.accent.with_alpha((self.accent.a as f32 * opacity) as u8));

        // Trophy circle with gradient effect (inner brighter)
        draw_circle(&mut pixmap, 38.0, h as f32 / 2.0, 20.0,
            self.accent.with_alpha((180.0 * opacity) as u8));
        draw_circle(&mut pixmap, 38.0, h as f32 / 2.0, 14.0,
            self.accent.with_alpha((220.0 * opacity) as u8));

        // Trophy icon (star character)
        rasterize_text(&mut pixmap, "\u{2605}", &self.display_font, 20.0, 28.0,
            h as f32 / 2.0 - 10.0, self.on_accent.with_alpha(opacity_byte));

        // "ACHIEVEMENT UNLOCKED" header
        rasterize_text(&mut pixmap, "ACHIEVEMENT UNLOCKED", &self.light_font, 10.0, 66.0,
            h as f32 / 2.0 - 26.0, self.fg.with_alpha(((opacity * 0.5) * 255.0) as u8));

        // Title
        rasterize_text(&mut pixmap, title, &self.display_font, 20.0, 66.0,
            h as f32 / 2.0 - 6.0, self.fg.with_alpha(opacity_byte));

        // Description
        if !description.is_empty() {
            rasterize_text(&mut pixmap, description, &self.light_font, 14.0, 66.0,
                h as f32 / 2.0 + 18.0, self.fg.with_alpha(((opacity * 0.7) * 255.0) as u8));
        }

        pixmap_to_argb(&pixmap)
    }

    // ── Menu rendering ──────────────────────────────────────

    pub fn render_menu(&self, menu: &Menu, screen_w: u32, screen_h: u32, config: &MenuConfig) -> Vec<u32> {
        let mut pixmap = Pixmap::new(screen_w, screen_h).expect("pixmap");
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let opacity = menu.opacity();
        if opacity <= 0.0 {
            return pixmap_to_argb(&pixmap);
        }

        // Backdrop
        let backdrop_a = (config.backdrop_opacity * opacity * 255.0) as u8;
        draw_rounded_rect(&mut pixmap, 0.0, 0.0, screen_w as f32, screen_h as f32, 0.0, self.bg.with_alpha(backdrop_a));

        // Panel dimensions
        let panel_w = config.width as f32;
        let title_height = 48.0;
        let items_height = menu.items().len() as f32 * config.item_height as f32;
        let panel_h = config.padding as f32 * 2.0 + title_height + items_height;

        let panel_x = (screen_w as f32 - panel_w) / 2.0;
        let panel_y = (screen_h as f32 - panel_h) / 2.0;

        // Scale animation (0.95 → 1.0)
        let scale = menu.scale();
        let scaled_panel_w = panel_w * scale;
        let scaled_panel_h = panel_h * scale;
        let scaled_x = (screen_w as f32 - scaled_panel_w) / 2.0;
        let scaled_y = (screen_h as f32 - scaled_panel_h) / 2.0;

        // Panel background
        let panel_a = (self.card.a as f32 * opacity) as u8;
        draw_rounded_rect(&mut pixmap, scaled_x, scaled_y, scaled_panel_w, scaled_panel_h, config.corner_radius * scale, self.card.with_alpha(panel_a));

        // Thin accent line at top of panel
        draw_rounded_rect(
            &mut pixmap,
            scaled_x,
            scaled_y,
            scaled_panel_w,
            3.0 * scale,
            config.corner_radius * scale,
            self.accent.with_alpha((self.accent.a as f32 * opacity) as u8),
        );

        // Title text
        let title_y = scaled_y + config.padding as f32 * scale + 32.0 * scale;
        let title_size = 26.0 * scale;
        let title_width = measure_text(&self.display_font, &config.title, title_size);
        let title_x = scaled_x + (scaled_panel_w - title_width) / 2.0;
        rasterize_text(&mut pixmap, &config.title, &self.display_font, title_size, title_x, title_y, self.fg.with_alpha((255.0 * opacity) as u8));

        // Items
        let items_start_y = scaled_y + config.padding as f32 * scale + title_height * scale;
        let item_h = config.item_height as f32 * scale;
        let item_text_size = 20.0 * scale;
        let item_padding_left = 24.0 * scale;
        let cursor = menu.cursor();
        let is_confirming = matches!(menu.state(), MenuState::Confirming { .. });

        for (i, item) in menu.items().iter().enumerate() {
            let item_y = items_start_y + i as f32 * item_h;
            let is_selected = i == cursor;

            if is_selected {
                // Selected item: accent background with rounded corners
                let sel_inset = 4.0 * scale;
                draw_rounded_rect(
                    &mut pixmap,
                    scaled_x + sel_inset,
                    item_y + 2.0 * scale,
                    scaled_panel_w - sel_inset * 2.0,
                    item_h - 4.0 * scale,
                    8.0 * scale,
                    self.accent.with_alpha((self.accent.a as f32 * opacity) as u8),
                );

                // Left accent bar
                draw_rounded_rect(
                    &mut pixmap,
                    scaled_x + sel_inset,
                    item_y + 6.0 * scale,
                    4.0 * scale,
                    item_h - 12.0 * scale,
                    2.0 * scale,
                    self.on_accent.with_alpha((200.0 * opacity) as u8),
                );

                // Label text
                let label = if is_confirming {
                    "Press again to confirm"
                } else {
                    &item.label
                };
                let text_y = item_y + item_h / 2.0 + item_text_size * 0.35;
                rasterize_text(&mut pixmap, label, &self.body_font, item_text_size, scaled_x + item_padding_left + 8.0 * scale, text_y, self.on_accent.with_alpha((255.0 * opacity) as u8));
            } else {
                // Normal item
                let text_y = item_y + item_h / 2.0 + item_text_size * 0.35;
                rasterize_text(&mut pixmap, &item.label, &self.body_font, item_text_size, scaled_x + item_padding_left, text_y, self.fg.with_alpha((204.0 * opacity) as u8)); // 80% of 255
            }
        }

        pixmap_to_argb(&pixmap)
    }
}

// ── Shared drawing helpers ──────────────────────────────────

fn draw_rounded_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();

    if let Some(path) = pb.finish() {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    let k = 0.5522847498_f32;
    let kr = k * radius;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - radius);
    pb.cubic_to(cx + kr, cy - radius, cx + radius, cy - kr, cx + radius, cy);
    pb.cubic_to(cx + radius, cy + kr, cx + kr, cy + radius, cx, cy + radius);
    pb.cubic_to(cx - kr, cy + radius, cx - radius, cy + kr, cx - radius, cy);
    pb.cubic_to(cx - radius, cy - kr, cx - kr, cy - radius, cx, cy - radius);
    pb.close();

    if let Some(path) = pb.finish() {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn rasterize_text(pixmap: &mut Pixmap, text: &str, font: &fontdue::Font, size: f32, x: f32, y: f32, c: Color8) {
    if c.a == 0 { return; }
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    let data = pixmap.data_mut();

    let mut cursor_x = x;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);
        let gx = cursor_x as i32 + metrics.xmin;
        let gy = y as i32 - metrics.height as i32 + (metrics.height as i32 - metrics.ymin);

        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let px = gx + col as i32;
                let py = gy + row as i32;
                if px < 0 || py < 0 || px >= pw || py >= ph { continue; }
                let coverage = bitmap[row * metrics.width + col];
                if coverage == 0 { continue; }
                let alpha = ((coverage as u32 * c.a as u32) / 255) as u8;
                let idx = ((py as u32 * pw as u32 + px as u32) * 4) as usize;
                let inv = 255 - alpha;
                data[idx] = ((c.r as u32 * alpha as u32 + data[idx] as u32 * inv as u32) / 255) as u8;
                data[idx + 1] = ((c.g as u32 * alpha as u32 + data[idx + 1] as u32 * inv as u32) / 255) as u8;
                data[idx + 2] = ((c.b as u32 * alpha as u32 + data[idx + 2] as u32 * inv as u32) / 255) as u8;
                data[idx + 3] = ((alpha as u32 + data[idx + 3] as u32 * inv as u32 / 255).min(255)) as u8;
            }
        }
        cursor_x += metrics.advance_width;
    }
}

fn measure_text(font: &fontdue::Font, text: &str, size: f32) -> f32 {
    text.chars().map(|ch| font.metrics(ch, size).advance_width).sum()
}

fn pixmap_to_argb(pixmap: &Pixmap) -> Vec<u32> {
    let data = pixmap.data();
    let mut argb = Vec::with_capacity(data.len() / 4);
    for chunk in data.chunks_exact(4) {
        argb.push((chunk[3] as u32) << 24 | (chunk[0] as u32) << 16 | (chunk[1] as u32) << 8 | chunk[2] as u32);
    }
    argb
}

fn load_font(path: &std::path::Path) -> fontdue::Font {
    match std::fs::read(path) {
        Ok(data) => {
            fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
                .unwrap_or_else(|e| {
                    log::warn!("Failed to parse font {}: {e}, using fallback", path.display());
                    fallback_font()
                })
        }
        Err(e) => {
            log::warn!("Failed to read font {}: {e}, using fallback", path.display());
            fallback_font()
        }
    }
}

fn fallback_font() -> fontdue::Font {
    panic!("No font available — ensure theme fonts exist at assets/fonts/");
}
