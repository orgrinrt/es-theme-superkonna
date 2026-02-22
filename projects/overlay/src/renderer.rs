//! Popup renderer using tiny-skia.
//! Draws themed achievement popups: rounded rect with accent border,
//! trophy icon area, title, and description text.

use crate::theme::Theme;
use tiny_skia::*;

pub struct Renderer {
    fg: Color8,
    bg: Color8,
    accent: Color8,
    width: u32,
    height: u32,
    title_font: fontdue::Font,
    body_font: fontdue::Font,
}

struct Color8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Renderer {
    pub fn new(theme: &Theme, width: u16, height: u16) -> Self {
        let title_font = load_font(&theme.font_path);
        let body_font = load_font(&theme.font_light_path);

        Renderer {
            fg: Color8 { r: theme.fg_color.r, g: theme.fg_color.g, b: theme.fg_color.b, a: theme.fg_color.a },
            bg: Color8 { r: theme.card_color.r, g: theme.card_color.g, b: theme.card_color.b, a: theme.card_color.a },
            accent: Color8 { r: theme.accent_color.r, g: theme.accent_color.g, b: theme.accent_color.b, a: theme.accent_color.a },
            width: width as u32,
            height: height as u32,
            title_font,
            body_font,
        }
    }

    /// Render a popup to ARGB pixel buffer.
    pub fn render_popup(&self, title: &str, description: &str, opacity: f32) -> Vec<u32> {
        let w = self.width;
        let h = self.height;
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");

        // Clear to transparent
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let opacity_byte = (opacity * 255.0) as u8;

        // Background rounded rect
        self.draw_rounded_rect(
            &mut pixmap,
            2.0,
            2.0,
            w as f32 - 4.0,
            h as f32 - 4.0,
            12.0,
            self.bg.r,
            self.bg.g,
            self.bg.b,
            (self.bg.a as f32 * opacity) as u8,
        );

        // Accent border (left edge strip)
        self.draw_rounded_rect(
            &mut pixmap,
            2.0,
            2.0,
            6.0,
            h as f32 - 4.0,
            3.0,
            self.accent.r,
            self.accent.g,
            self.accent.b,
            (self.accent.a as f32 * opacity) as u8,
        );

        // Trophy icon area (placeholder: filled circle)
        self.draw_circle(
            &mut pixmap,
            36.0,
            h as f32 / 2.0,
            18.0,
            self.accent.r,
            self.accent.g,
            self.accent.b,
            (200.0 * opacity) as u8,
        );

        // Title text
        self.rasterize_text(
            &mut pixmap,
            title,
            &self.title_font,
            22.0,
            64.0,
            h as f32 / 2.0 - 14.0,
            self.fg.r,
            self.fg.g,
            self.fg.b,
            opacity_byte,
        );

        // Description text
        if !description.is_empty() {
            self.rasterize_text(
                &mut pixmap,
                description,
                &self.body_font,
                16.0,
                64.0,
                h as f32 / 2.0 + 14.0,
                self.fg.r,
                self.fg.g,
                self.fg.b,
                ((opacity * 0.7) * 255.0) as u8,
            );
        }

        // Convert pixmap (RGBA premultiplied) to ARGB u32 array
        let data = pixmap.data();
        let mut argb = Vec::with_capacity((w * h) as usize);
        for chunk in data.chunks_exact(4) {
            let r = chunk[0];
            let g = chunk[1];
            let b = chunk[2];
            let a = chunk[3];
            argb.push((a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
        }
        argb
    }

    fn draw_rounded_rect(
        &self,
        pixmap: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
        let mut pb = PathBuilder::new();
        // Simple rounded rect via arcs approximation
        pb.move_to(x + radius, y);
        pb.line_to(x + w - radius, y);
        pb.quad_to(x + w, y, x + w, y + radius);
        pb.line_to(x + w, y + h - radius);
        pb.quad_to(x + w, y + h, x + w - radius, y + h);
        pb.line_to(x + radius, y + h);
        pb.quad_to(x, y + h, x, y + h - radius);
        pb.line_to(x, y + radius);
        pb.quad_to(x, y, x + radius, y);
        pb.close();

        if let Some(path) = pb.finish() {
            let paint = Paint {
                shader: Shader::SolidColor(
                    tiny_skia::Color::from_rgba8(r, g, b, a),
                ),
                anti_alias: true,
                ..Paint::default()
            };
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    fn draw_circle(
        &self,
        pixmap: &mut Pixmap,
        cx: f32,
        cy: f32,
        radius: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
        // Approximate circle with cubic beziers
        let k = 0.5522847498; // (4/3)*tan(pi/8)
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
                shader: Shader::SolidColor(
                    tiny_skia::Color::from_rgba8(r, g, b, a),
                ),
                anti_alias: true,
                ..Paint::default()
            };
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    fn rasterize_text(
        &self,
        pixmap: &mut Pixmap,
        text: &str,
        font: &fontdue::Font,
        size: f32,
        x: f32,
        y: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
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
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 {
                        continue;
                    }
                    let alpha = ((coverage as u32 * a as u32) / 255) as u8;
                    let idx = ((py as u32 * pw as u32 + px as u32) * 4) as usize;
                    // Alpha blend (premultiplied)
                    let inv = 255 - alpha;
                    data[idx] = ((r as u32 * alpha as u32 + data[idx] as u32 * inv as u32) / 255) as u8;
                    data[idx + 1] = ((g as u32 * alpha as u32 + data[idx + 1] as u32 * inv as u32) / 255) as u8;
                    data[idx + 2] = ((b as u32 * alpha as u32 + data[idx + 2] as u32 * inv as u32) / 255) as u8;
                    data[idx + 3] = ((alpha as u32 + data[idx + 3] as u32 * inv as u32 / 255).min(255)) as u8;
                }
            }
            cursor_x += metrics.advance_width;
        }
    }
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
    // Minimal built-in font data is not practical, so we panic here.
    // In production, the theme fonts should always be available.
    panic!("No font available — ensure theme fonts exist at assets/fonts/");
}
