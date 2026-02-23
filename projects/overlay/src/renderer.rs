//! Widget-based overlay renderer — PS5 / Steam Deck inspired.
//!
//! Independent screen-edge widgets composited into a fullscreen ARGB buffer.
//! Premium console aesthetic: layered shadows, gradient panels, glow accents,
//! bold selection pills, controller face buttons.

use crate::config::MenuConfig;
use crate::menu::{Menu, MenuState};
use crate::popup::Popup;
use crate::theme::Theme;
use tiny_skia::*;

// ── Layout constants ────────────────────────────────────────

// Toast (achievement notification, top-right)
const TOAST_W: f32 = 440.0;
const TOAST_H: f32 = 96.0;
const TOAST_RADIUS: f32 = 16.0;
const TOAST_MARGIN: f32 = 28.0;
const TOAST_BADGE_SIZE: f32 = 60.0;
const TOAST_BADGE_RADIUS: f32 = 12.0;
const TOAST_BADGE_PAD: f32 = 18.0;

// Menu panel (left side)
const MENU_WIDTH: f32 = 320.0;
const MENU_MARGIN: f32 = 48.0;
const MENU_RADIUS: f32 = 20.0;
const MENU_ITEM_H: f32 = 56.0;
const MENU_HINT_H: f32 = 44.0;
const MENU_PAD: f32 = 16.0;
const MENU_ITEM_INSET: f32 = 10.0;
const MENU_SEL_RADIUS: f32 = 12.0;

// Status pill (top-left)
const STATUS_H: f32 = 32.0;
const STATUS_RADIUS: f32 = 16.0;
const STATUS_MARGIN: f32 = 28.0;
const STATUS_PAD_H: f32 = 16.0;

// Backdrop
const BACKDROP_ALPHA: f32 = 0.45;

// Shadow layers — offset, spread, opacity
const SHADOW_LAYERS: [(f32, f32, u8); 3] = [
    (0.0, 32.0, 30),  // ambient
    (4.0, 16.0, 45),  // medium
    (2.0, 4.0, 60),   // tight
];

pub struct Renderer {
    fg: Color8,
    bg: Color8,
    accent: Color8,
    on_accent: Color8,
    sect: Color8,
    card: Color8,
    shadow: Color8,
    subtle: Color8,
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

    fn blend(self, other: Color8, t: f32) -> Self {
        let inv = 1.0 - t;
        Color8 {
            r: (self.r as f32 * inv + other.r as f32 * t) as u8,
            g: (self.g as f32 * inv + other.g as f32 * t) as u8,
            b: (self.b as f32 * inv + other.b as f32 * t) as u8,
            a: (self.a as f32 * inv + other.a as f32 * t) as u8,
        }
    }
}

/// All state needed to render one frame.
pub struct FrameState<'a> {
    pub popup: Option<&'a Popup>,
    pub menu: Option<&'a Menu>,
    pub menu_config: &'a MenuConfig,
    pub game_name: Option<&'a str>,
}

impl Renderer {
    pub fn new(theme: &Theme) -> Self {
        Renderer {
            fg: Color8::from_theme(&theme.fg_color),
            bg: Color8::from_theme(&theme.bg_color),
            accent: Color8::from_theme(&theme.accent_color),
            on_accent: Color8::from_theme(&theme.on_accent_color),
            sect: Color8::from_theme(&theme.sect_color),
            card: Color8::from_theme(&theme.card_color),
            shadow: Color8::from_theme(&theme.shadow_color),
            subtle: Color8::from_theme(&theme.subtle_color),
            display_font: load_font(&theme.font_display_path),
            body_font: load_font(&theme.font_path),
            light_font: load_font(&theme.font_light_path),
        }
    }

    // ── Main entry point ────────────────────────────────────

    /// Render all visible widgets into a single fullscreen ARGB buffer.
    pub fn render_frame(&self, state: &FrameState, screen_w: u32, screen_h: u32) -> Vec<u32> {
        let mut pixmap = Pixmap::new(screen_w, screen_h).expect("pixmap");
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let menu_visible = state.menu.map_or(false, |m| m.is_visible());
        let menu_opacity = state.menu.map_or(0.0, |m| m.opacity());

        // Backdrop vignette (only when menu is open)
        if menu_visible {
            self.draw_backdrop(&mut pixmap, screen_w, screen_h, menu_opacity);
        }

        // Status pill (top-left, only when menu is open)
        if menu_visible {
            self.draw_status_pill(&mut pixmap, state.game_name, menu_opacity);
        }

        // Quick menu (left side)
        if let Some(menu) = state.menu {
            if menu.is_visible() {
                self.draw_menu_panel(&mut pixmap, menu, state.menu_config, screen_h);
            }
        }

        // Achievement toast (top-right, slides from right)
        if let Some(popup) = state.popup {
            self.draw_achievement_toast(&mut pixmap, popup, screen_w);
        }

        pixmap_to_argb(&pixmap)
    }

    // ── Legacy API ──────────────────────────────────────────

    pub fn render_popup(&self, title: &str, description: &str, opacity: f32) -> Vec<u32> {
        let w = TOAST_W as u32 + TOAST_MARGIN as u32 * 2;
        let h = TOAST_H as u32 + TOAST_MARGIN as u32 * 2;
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let popup = Popup::new(title.to_string(), description.to_string());
        self.draw_toast_at(&mut pixmap, TOAST_MARGIN, TOAST_MARGIN, &popup, opacity, 0.0);
        pixmap_to_argb(&pixmap)
    }

    pub fn render_menu(&self, menu: &Menu, screen_w: u32, screen_h: u32, config: &MenuConfig) -> Vec<u32> {
        let state = FrameState {
            popup: None,
            menu: Some(menu),
            menu_config: config,
            game_name: Some("Preview Game"),
        };
        self.render_frame(&state, screen_w, screen_h)
    }

    // ── Backdrop ────────────────────────────────────────────

    fn draw_backdrop(&self, pixmap: &mut Pixmap, w: u32, h: u32, opacity: f32) {
        // Radial-ish vignette: darker at edges, slightly lighter center
        let base_a = (BACKDROP_ALPHA * opacity * 255.0) as u8;
        let edge_a = ((BACKDROP_ALPHA + 0.15) * opacity * 255.0).min(255.0) as u8;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let max_dist = (cx * cx + cy * cy).sqrt();

        // Draw as horizontal bands for efficiency (approximate radial gradient)
        let band = 12_u32;
        for by in (0..h).step_by(band as usize) {
            let dy = (by as f32 + band as f32 / 2.0 - cy).abs() / cy;
            let t = dy.min(1.0);
            let a = base_a as f32 + (edge_a as f32 - base_a as f32) * t * t;
            fill_rect(pixmap, 0.0, by as f32, w as f32, band as f32, self.bg.with_alpha(a as u8));
        }
        let _ = (cx, max_dist); // suppress unused
    }

    // ── Achievement toast ───────────────────────────────────

    fn draw_achievement_toast(&self, pixmap: &mut Pixmap, popup: &Popup, screen_w: u32) {
        let opacity = popup.opacity();
        if opacity <= 0.0 { return; }

        let slide = popup.slide_offset();
        let x = screen_w as f32 - TOAST_W - TOAST_MARGIN + (TOAST_W + TOAST_MARGIN) * slide;
        let y = TOAST_MARGIN;

        self.draw_toast_at(pixmap, x, y, popup, opacity, slide);
    }

    fn draw_toast_at(&self, pixmap: &mut Pixmap, x: f32, y: f32, popup: &Popup, opacity: f32, _slide: f32) {
        let oa = |base: u8| -> u8 { (base as f32 * opacity) as u8 };

        // Drop shadows (3 layers for depth) — use theme shadow color
        for &(offset, spread, alpha) in &SHADOW_LAYERS {
            let sa = oa(alpha);
            if sa == 0 { continue; }
            let s = spread / 2.0;
            draw_rounded_rect(pixmap,
                x - s + offset, y - s + offset * 1.5,
                TOAST_W + spread, TOAST_H + spread,
                TOAST_RADIUS + s,
                self.shadow.with_alpha(sa));
        }

        // Panel background gradient — card color, lighter at top via subtle
        let bg_top = self.card.blend(self.subtle, 0.06).with_alpha(oa(235));
        let bg_bot = self.card.with_alpha(oa(245));
        draw_gradient_rounded_rect(pixmap, x, y, TOAST_W, TOAST_H, TOAST_RADIUS, bg_top, bg_bot);

        // Outer border — subtle edge definition
        draw_rounded_rect_stroke(pixmap, x, y, TOAST_W, TOAST_H, TOAST_RADIUS,
            self.subtle.with_alpha(oa(8)));

        // Top inner highlight
        draw_rounded_rect(pixmap, x + 1.0, y + 1.0, TOAST_W - 2.0, 1.0, TOAST_RADIUS - 1.0,
            self.subtle.with_alpha(oa(15)));

        // Left accent glow (bleed + solid stripe)
        draw_rounded_rect(pixmap, x + 1.0, y + 12.0, 8.0, TOAST_H - 24.0, 4.0,
            self.accent.with_alpha(oa(15)));
        draw_rounded_rect(pixmap, x + 2.0, y + 14.0, 3.0, TOAST_H - 28.0, 1.5,
            self.accent.with_alpha(oa(220)));

        // Badge area
        let badge_x = x + TOAST_BADGE_PAD;
        let badge_y = y + (TOAST_H - TOAST_BADGE_SIZE) / 2.0;

        let has_badge = if let Some(ref png_bytes) = popup.badge_png {
            self.blit_badge(pixmap, badge_x, badge_y, TOAST_BADGE_SIZE, TOAST_BADGE_RADIUS, png_bytes, opacity)
        } else {
            false
        };

        if !has_badge {
            // Shadow behind badge
            draw_rounded_rect(pixmap,
                badge_x + 2.0, badge_y + 3.0,
                TOAST_BADGE_SIZE, TOAST_BADGE_SIZE, TOAST_BADGE_RADIUS,
                self.shadow.with_alpha(oa(50)));
            // Badge bg — accent gradient darkened with shadow color
            let badge_top = self.accent.with_alpha(oa(220));
            let badge_bot = self.accent.blend(self.shadow, 0.25).with_alpha(oa(220));
            draw_gradient_rounded_rect(pixmap,
                badge_x, badge_y, TOAST_BADGE_SIZE, TOAST_BADGE_SIZE,
                TOAST_BADGE_RADIUS, badge_top, badge_bot);
            // Badge inner highlight
            draw_rounded_rect(pixmap,
                badge_x + 1.0, badge_y + 1.0,
                TOAST_BADGE_SIZE - 2.0, TOAST_BADGE_SIZE * 0.4,
                TOAST_BADGE_RADIUS - 1.0,
                self.subtle.with_alpha(oa(30)));
            // Star centered in badge
            let star_size = 24.0;
            let sx = badge_x + (TOAST_BADGE_SIZE - measure_text(&self.display_font, "\u{2605}", star_size)) / 2.0;
            let sy = text_center_y(&self.display_font, star_size, badge_y, TOAST_BADGE_SIZE);
            rasterize_text(pixmap, "\u{2605}", &self.display_font, star_size,
                sx, sy, self.on_accent.with_alpha(oa(240)));
        }

        // Badge border ring
        draw_rounded_rect_stroke(pixmap,
            badge_x, badge_y, TOAST_BADGE_SIZE, TOAST_BADGE_SIZE,
            TOAST_BADGE_RADIUS, self.accent.with_alpha(oa(60)));

        // Text column — 3 lines vertically distributed in toast
        let text_x = badge_x + TOAST_BADGE_SIZE + 16.0;
        let text_max_w = TOAST_W - (text_x - x) - 16.0;

        // Vertical layout: header(9.5) + title(16) + desc(11.5) with gaps
        let header_size = 9.5_f32;
        let title_size = 16.0_f32;
        let desc_size = 11.5_f32;
        let header_h = text_height(&self.body_font, header_size);
        let title_h = text_height(&self.display_font, title_size);
        let desc_h = text_height(&self.light_font, desc_size);
        let line_gap = 2.0;
        let has_desc = !popup.description.is_empty();
        let total_text_h = header_h + line_gap + title_h + if has_desc { line_gap + desc_h } else { 0.0 };
        let text_top = y + (TOAST_H - total_text_h) / 2.0;

        // "ACHIEVEMENT UNLOCKED" header
        rasterize_text(pixmap, "ACHIEVEMENT UNLOCKED", &self.body_font, header_size,
            text_x, text_top, self.accent.with_alpha(oa(200)));

        // Title
        let title_y = text_top + header_h + line_gap;
        let title_trunc = truncate_to_width(&self.display_font, &popup.title, title_size, text_max_w);
        rasterize_text(pixmap, &title_trunc, &self.display_font, title_size,
            text_x, title_y, self.fg.with_alpha(oa(250)));

        // Description
        if has_desc {
            let desc_y = title_y + title_h + line_gap;
            let desc_trunc = truncate_to_width(&self.light_font, &popup.description, desc_size, text_max_w);
            rasterize_text(pixmap, &desc_trunc, &self.light_font, desc_size,
                text_x, desc_y, self.subtle.with_alpha(oa(120)));
        }
    }

    /// Decode PNG bytes and blit as a rounded badge.
    fn blit_badge(&self, pixmap: &mut Pixmap, x: f32, y: f32, size: f32, radius: f32, png_bytes: &[u8], opacity: f32) -> bool {
        let Ok(decoder) = png::Decoder::new(std::io::Cursor::new(png_bytes)).read_info() else {
            return false;
        };
        let info = decoder.info().clone();
        let mut reader = decoder;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let Ok(_output_info) = reader.next_frame(&mut buf) else {
            return false;
        };

        let src_w = info.width as usize;
        let src_h = info.height as usize;
        let channels = match info.color_type {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            _ => return false,
        };

        let dst_size = size as usize;
        let pw = pixmap.width() as usize;
        let ph = pixmap.height() as usize;
        let data = pixmap.data_mut();

        for dy in 0..dst_size {
            for dx in 0..dst_size {
                if !in_rounded_rect(dx as f32, dy as f32, size, size, radius) {
                    continue;
                }

                let sx = (dx * src_w / dst_size).min(src_w - 1);
                let sy = (dy * src_h / dst_size).min(src_h - 1);
                let si = (sy * src_w + sx) * channels;
                if si + 2 >= buf.len() { continue; }

                let sr = buf[si];
                let sg = buf[si + 1];
                let sb = buf[si + 2];
                let sa = if channels == 4 { buf[si + 3] } else { 255 };
                let alpha = (sa as f32 * opacity) as u8;
                if alpha == 0 { continue; }

                let px = x as usize + dx;
                let py = y as usize + dy;
                if px >= pw || py >= ph { continue; }

                let di = (py * pw + px) * 4;
                let inv = 255u16 - alpha as u16;
                data[di] = ((sr as u16 * alpha as u16 + data[di] as u16 * inv) / 255) as u8;
                data[di + 1] = ((sg as u16 * alpha as u16 + data[di + 1] as u16 * inv) / 255) as u8;
                data[di + 2] = ((sb as u16 * alpha as u16 + data[di + 2] as u16 * inv) / 255) as u8;
                data[di + 3] = (alpha as u16 + data[di + 3] as u16 * inv / 255).min(255) as u8;
            }
        }
        true
    }

    // ── Quick menu panel ────────────────────────────────────

    fn draw_menu_panel(&self, pixmap: &mut Pixmap, menu: &Menu, config: &MenuConfig, screen_h: u32) {
        let opacity = menu.opacity();
        if opacity <= 0.0 { return; }
        let oa = |base: u8| -> u8 { (base as f32 * opacity) as u8 };
        let scale = menu.scale();

        let n_items = menu.items().len() as f32;
        let top_pad = 20.0;
        let panel_h = top_pad + n_items * MENU_ITEM_H + MENU_HINT_H + MENU_PAD * 2.0;
        let panel_w = MENU_WIDTH * scale;
        let panel_h_scaled = panel_h * scale;

        // Slide from left
        let slide_t = (1.0 - opacity).max(0.0);
        let panel_x = MENU_MARGIN - (MENU_WIDTH * 0.3 * slide_t);
        let panel_y = (screen_h as f32 - panel_h_scaled) / 2.0;

        // Drop shadows — use theme shadow color
        for &(offset, spread, alpha) in &SHADOW_LAYERS {
            let sa = oa(alpha);
            if sa == 0 { continue; }
            let s = spread / 2.0;
            draw_rounded_rect(pixmap,
                panel_x - s + offset, panel_y - s + offset * 1.5,
                panel_w + spread, panel_h_scaled + spread,
                MENU_RADIUS * scale + s,
                self.shadow.with_alpha(sa));
        }

        // Panel gradient background — card color, lighter at top via subtle
        let panel_top = self.card.blend(self.subtle, 0.05).with_alpha(oa(240));
        let panel_bot = self.card.with_alpha(oa(248));
        draw_gradient_rounded_rect(pixmap, panel_x, panel_y,
            panel_w, panel_h_scaled, MENU_RADIUS * scale, panel_top, panel_bot);

        // Outer border
        draw_rounded_rect_stroke(pixmap, panel_x, panel_y,
            panel_w, panel_h_scaled, MENU_RADIUS * scale,
            self.subtle.with_alpha(oa(6)));

        // Top inner highlight
        draw_rounded_rect(pixmap, panel_x + 1.0, panel_y + 1.0,
            panel_w - 2.0, 1.0, MENU_RADIUS * scale - 1.0,
            self.subtle.with_alpha(oa(12)));

        // Menu items (no header text — the panel IS the menu)
        let items_y = panel_y + top_pad * scale + MENU_PAD * scale;
        let item_h = MENU_ITEM_H * scale;
        let item_text_size = 15.0 * scale;
        let cursor = menu.cursor();
        let is_confirming = matches!(menu.state(), MenuState::Confirming { .. });

        for (i, item) in menu.items().iter().enumerate() {
            let iy = items_y + i as f32 * item_h;
            let is_selected = i == cursor;

            if is_selected {
                // Selected: full accent pill
                let sel_x = panel_x + MENU_ITEM_INSET * scale;
                let sel_w = panel_w - MENU_ITEM_INSET * scale * 2.0;
                let sel_y = iy + 3.0 * scale;
                let sel_h = item_h - 6.0 * scale;

                // Glow behind selection
                draw_rounded_rect(pixmap,
                    sel_x - 2.0, sel_y - 1.0, sel_w + 4.0, sel_h + 2.0,
                    MENU_SEL_RADIUS * scale + 2.0,
                    self.accent.with_alpha(oa(25)));

                // Selection pill — accent gradient, darkened with shadow
                let pill_top = self.accent.with_alpha(oa(200));
                let pill_bot = self.accent.blend(self.shadow, 0.2).with_alpha(oa(200));
                draw_gradient_rounded_rect(pixmap,
                    sel_x, sel_y, sel_w, sel_h,
                    MENU_SEL_RADIUS * scale, pill_top, pill_bot);

                // Inner highlight on pill
                draw_rounded_rect(pixmap,
                    sel_x + 1.0, sel_y + 1.0,
                    sel_w - 2.0, sel_h * 0.35,
                    MENU_SEL_RADIUS * scale - 1.0,
                    self.subtle.with_alpha(oa(18)));

                let label = if is_confirming {
                    "Press again to confirm"
                } else {
                    &item.label
                };
                let text_color = if is_confirming {
                    self.subtle.blend(self.accent, 0.3).with_alpha(oa(255))
                } else {
                    self.on_accent.with_alpha(oa(255))
                };
                let text_y = text_center_y(&self.body_font, item_text_size, iy, item_h);
                // Center text in pill
                let tw = measure_text(&self.body_font, label, item_text_size);
                let tx = sel_x + (sel_w - tw) / 2.0;
                rasterize_text(pixmap, label, &self.body_font, item_text_size,
                    tx, text_y, text_color);
            } else {
                let text_y = text_center_y(&self.body_font, item_text_size, iy, item_h);
                let tw = measure_text(&self.body_font, &item.label, item_text_size);
                let tx = panel_x + (panel_w - tw) / 2.0;
                rasterize_text(pixmap, &item.label, &self.body_font, item_text_size,
                    tx, text_y, self.fg.with_alpha(oa(140)));
            }

            // NO divider lines — spacing alone separates items
        }

        // Hint bar (bottom)
        let hint_y = items_y + n_items * item_h + 4.0 * scale;
        // Subtle separator
        fill_rect(pixmap,
            panel_x + MENU_PAD * scale * 2.0, hint_y,
            panel_w - MENU_PAD * scale * 4.0, 1.0,
            self.subtle.with_alpha(oa(8)));

        let hint_size = 10.0 * scale;
        let hint_center_y = hint_y + MENU_HINT_H * scale * 0.5;

        // Controller face buttons as circles (PS-style)
        let total_hint_w = self.measure_hints(hint_size, scale);
        let mut hx = panel_x + (panel_w - total_hint_w) / 2.0; // center hints

        hx = self.draw_face_button(pixmap, hx, hint_center_y, "A", "Select", hint_size, scale, opacity, true);
        hx += 16.0 * scale;
        let _ = self.draw_face_button(pixmap, hx, hint_center_y, "B", "Back", hint_size, scale, opacity, false);
    }

    fn measure_hints(&self, size: f32, scale: f32) -> f32 {
        let btn_d = size + 8.0 * scale;
        let gap = 4.0 * scale;
        let sep = 16.0 * scale;
        let a_label = measure_text(&self.light_font, "Select", size);
        let b_label = measure_text(&self.light_font, "Back", size);
        btn_d + gap + a_label + sep + btn_d + gap + b_label
    }

    fn draw_face_button(&self, pixmap: &mut Pixmap, x: f32, cy: f32, button: &str, label: &str, size: f32, scale: f32, opacity: f32, is_primary: bool) -> f32 {
        let oa = |base: u8| -> u8 { (base as f32 * opacity) as u8 };
        let btn_d = size + 8.0 * scale; // circle diameter
        let btn_r = btn_d / 2.0;
        let bcx = x + btn_r;
        let bcy = cy;

        // Circle background
        let bg_color = if is_primary {
            self.accent.with_alpha(oa(60))
        } else {
            self.subtle.with_alpha(oa(20))
        };
        draw_circle(pixmap, bcx, bcy, btn_r, bg_color);

        // Circle border
        let border_color = if is_primary {
            self.accent.with_alpha(oa(100))
        } else {
            self.subtle.with_alpha(oa(30))
        };
        draw_circle_stroke(pixmap, bcx, bcy, btn_r, border_color);

        // Letter centered in circle
        let lw = measure_text(&self.body_font, button, size * 0.85);
        let letter_y = text_center_y(&self.body_font, size * 0.85, bcy - btn_r, btn_d);
        rasterize_text(pixmap, button, &self.body_font, size * 0.85,
            bcx - lw / 2.0, letter_y,
            self.fg.with_alpha(oa(200)));

        // Label text centered with circle
        let gap = 4.0 * scale;
        let lx = x + btn_d + gap;
        let label_y = text_center_y(&self.light_font, size, bcy - btn_r, btn_d);
        rasterize_text(pixmap, label, &self.light_font, size,
            lx, label_y,
            self.fg.with_alpha(oa(80)));

        lx + measure_text(&self.light_font, label, size)
    }

    // ── Status pill ─────────────────────────────────────────

    fn draw_status_pill(&self, pixmap: &mut Pixmap, game_name: Option<&str>, opacity: f32) {
        let oa = |base: u8| -> u8 { (base as f32 * opacity) as u8 };

        let clock = {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let hours = ((now % 86400) / 3600) as u32;
            let minutes = ((now % 3600) / 60) as u32;
            format!("{:02}:{:02}", hours, minutes)
        };

        let text = match game_name {
            Some(name) => format!("{}  \u{00B7}  {}", clock, name),
            None => clock,
        };

        let text_size = 12.0_f32;
        let text_w = measure_text(&self.light_font, &text, text_size);
        let pill_w = text_w + STATUS_PAD_H * 2.0;
        let x = STATUS_MARGIN;
        let y = STATUS_MARGIN;

        // Shadow
        draw_rounded_rect(pixmap, x + 1.0, y + 2.0, pill_w, STATUS_H, STATUS_RADIUS,
            self.shadow.with_alpha(oa(40)));

        // Glass pill
        draw_rounded_rect(pixmap, x, y, pill_w, STATUS_H, STATUS_RADIUS,
            self.card.with_alpha(oa(220)));

        // Border glow (accent tinted)
        draw_rounded_rect_stroke(pixmap, x, y, pill_w, STATUS_H, STATUS_RADIUS,
            self.accent.with_alpha(oa(25)));

        // Top highlight
        draw_rounded_rect(pixmap, x + 1.0, y + 1.0, pill_w - 2.0, 1.0, STATUS_RADIUS - 1.0,
            self.subtle.with_alpha(oa(12)));

        // Text vertically centered in pill
        let text_y = text_center_y(&self.light_font, text_size, y, STATUS_H);
        rasterize_text(pixmap, &text, &self.light_font, text_size,
            x + STATUS_PAD_H, text_y,
            self.fg.with_alpha(oa(170)));
    }
}

// ── Shared drawing helpers ──────────────────────────────────

fn fill_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Color8) {
    draw_rounded_rect(pixmap, x, y, w, h, 0.0, c);
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
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
    pb.finish()
}

fn draw_rounded_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    if let Some(path) = rounded_rect_path(x, y, w, h, radius) {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// Vertical gradient fill (top_color at y, bottom_color at y+h).
fn draw_gradient_rounded_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32, top: Color8, bot: Color8) {
    if top.a == 0 && bot.a == 0 { return; }
    if let Some(path) = rounded_rect_path(x, y, w, h, radius) {
        let stops = vec![
            GradientStop::new(0.0, tiny_skia::Color::from_rgba8(top.r, top.g, top.b, top.a)),
            GradientStop::new(1.0, tiny_skia::Color::from_rgba8(bot.r, bot.g, bot.b, bot.a)),
        ];
        if let Some(shader) = LinearGradient::new(
            Point::from_xy(x, y),
            Point::from_xy(x, y + h),
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            let paint = Paint {
                shader,
                anti_alias: true,
                ..Paint::default()
            };
            pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// Stroke outline of a rounded rect (1px).
fn draw_rounded_rect_stroke(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    if let Some(path) = rounded_rect_path(x, y, w, h, radius) {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn circle_path(cx: f32, cy: f32, radius: f32) -> Option<Path> {
    let k = 0.5522847498_f32;
    let kr = k * radius;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - radius);
    pb.cubic_to(cx + kr, cy - radius, cx + radius, cy - kr, cx + radius, cy);
    pb.cubic_to(cx + radius, cy + kr, cx + kr, cy + radius, cx, cy + radius);
    pb.cubic_to(cx - kr, cy + radius, cx - radius, cy + kr, cx - radius, cy);
    pb.cubic_to(cx - radius, cy - kr, cx - kr, cy - radius, cx, cy - radius);
    pb.close();
    pb.finish()
}

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    if let Some(path) = circle_path(cx, cy, radius) {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// Stroke outline of a circle (1px).
fn draw_circle_stroke(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, c: Color8) {
    if c.a == 0 { return; }
    if let Some(path) = circle_path(cx, cy, radius) {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)),
            anti_alias: true,
            ..Paint::default()
        };
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// Check if a point is inside a rounded rect (0,0,w,h) with given radius.
fn in_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> bool {
    if r <= 0.0 { return true; }
    // Check four corners
    let corners = [(r, r), (w - r, r), (r, h - r), (w - r, h - r)];
    for &(cx, cy) in &corners {
        let dx = if x < cx && cx < r + 0.5 { cx - x }
                 else if x > cx && cx > w - r - 0.5 { x - cx }
                 else { 0.0 };
        let dy = if y < cy && cy < r + 0.5 { cy - y }
                 else if y > cy && cy > h - r - 0.5 { y - cy }
                 else { 0.0 };
        if dx > 0.0 && dy > 0.0 && dx * dx + dy * dy > r * r {
            return false;
        }
    }
    true
}

/// Rasterize text with y = top of text em-box (not baseline).
/// Computes baseline internally from font ascent metrics.
fn rasterize_text(pixmap: &mut Pixmap, text: &str, font: &fontdue::Font, size: f32, x: f32, y: f32, c: Color8) {
    if c.a == 0 { return; }
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    let data = pixmap.data_mut();

    let ascent = font.horizontal_line_metrics(size)
        .map(|lm| lm.ascent)
        .unwrap_or(size * 0.8);
    let baseline_y = y + ascent;

    let mut cursor_x = x;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, size);
        let gx = cursor_x as i32 + metrics.xmin;
        let gy = baseline_y as i32 - metrics.ymin - metrics.height as i32;

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

/// Height of the font em-box (ascent - descent) for vertical centering.
fn text_height(font: &fontdue::Font, size: f32) -> f32 {
    font.horizontal_line_metrics(size)
        .map(|lm| lm.ascent - lm.descent)
        .unwrap_or(size)
}

/// Compute y (top of em-box) to vertically center text within a container.
fn text_center_y(font: &fontdue::Font, size: f32, container_y: f32, container_h: f32) -> f32 {
    container_y + (container_h - text_height(font, size)) / 2.0
}

pub fn measure_text(font: &fontdue::Font, text: &str, size: f32) -> f32 {
    text.chars().map(|ch| font.metrics(ch, size).advance_width).sum()
}

pub fn truncate_to_width(font: &fontdue::Font, text: &str, size: f32, max_width: f32) -> String {
    let mut result = String::new();
    let mut width = 0.0;
    let ellipsis_width = measure_text(font, "...", size);
    for ch in text.chars() {
        let cw = font.metrics(ch, size).advance_width;
        if width + cw + ellipsis_width > max_width && !result.is_empty() {
            result.push_str("...");
            return result;
        }
        width += cw;
        result.push(ch);
    }
    result
}

pub fn pixmap_to_argb(pixmap: &Pixmap) -> Vec<u32> {
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
