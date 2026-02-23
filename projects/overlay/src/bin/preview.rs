//! Local preview tool — renders overlay widgets to PNG files + atlas.
//! No X11 needed; runs on macOS/Linux/Windows.
//!
//! Usage: cargo run --bin preview [-- --theme-root PATH]
//!
//! Outputs:
//!   preview-output/toast-*.png      — achievement toast variants
//!   preview-output/menu-*.png       — menu panel at each cursor position
//!   preview-output/combined-*.png   — full frame with all widgets composited
//!   preview-output/atlas.png        — single tiled overview

use std::path::PathBuf;

use superkonna_overlay::config::OverlayConfig;
use superkonna_overlay::menu::Menu;
use superkonna_overlay::popup::Popup;
use superkonna_overlay::renderer::{FrameState, Renderer};
use superkonna_overlay::theme::Theme;

const SCREEN_W: u32 = 1280;
const SCREEN_H: u32 = 720;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let theme_root = std::env::args()
        .skip_while(|a| a != "--theme-root")
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest.parent().unwrap().parent().unwrap().to_path_buf()
        });

    println!("theme root: {}", theme_root.display());

    let theme = Theme::load(&theme_root).expect("failed to load theme");
    let rend = Renderer::new(&theme);
    let config = OverlayConfig::find_and_load(&theme_root);

    let out = PathBuf::from("preview-output");
    std::fs::create_dir_all(&out).unwrap();

    let mut all_frames: Vec<(String, Vec<u32>, u32, u32)> = Vec::new();

    // ── Toast-only frames ───────────────────────────────────
    let toasts = [
        ("First Blood", "Defeat the first enemy"),
        ("Speed Demon", "Complete level 1 in under 60 seconds"),
        ("Completionist", "Collect all 120 stars across all worlds"),
        ("Dragon Slayer Supreme", "A very long description that should truncate with an ellipsis automatically"),
    ];

    // Generate a fake game background (gradient simulating a game scene)
    let game_bg = generate_game_background(SCREEN_W, SCREEN_H);

    for (i, (title, desc)) in toasts.iter().enumerate() {
        let mut popup = Popup::new(title.to_string(), desc.to_string());
        popup.force_hold();
        let state = FrameState {
            popup: Some(&popup),
            menu: None,
            menu_config: &config.menu,
            game_name: None,
        };
        let argb = rend.render_frame(&state, SCREEN_W, SCREEN_H);
        let composited = composite_over_bg(&game_bg, &argb, SCREEN_W, SCREEN_H);
        let label = format!("toast-{}", i);
        save_argb_png(&out.join(format!("{}.png", label)), SCREEN_W, SCREEN_H, &composited);
        all_frames.push((label, composited, SCREEN_W, SCREEN_H));
    }

    // Toast with placeholder badge
    {
        let badge_png = generate_placeholder_badge(56, 56);
        let mut popup = Popup::new("Badge Test".to_string(), "With actual badge image".to_string())
            .with_badge(badge_png);
        popup.force_hold();
        let state = FrameState {
            popup: Some(&popup),
            menu: None,
            menu_config: &config.menu,
            game_name: None,
        };
        let argb = rend.render_frame(&state, SCREEN_W, SCREEN_H);
        let composited = composite_over_bg(&game_bg, &argb, SCREEN_W, SCREEN_H);
        let label = "toast-badge".to_string();
        save_argb_png(&out.join(format!("{}.png", label)), SCREEN_W, SCREEN_H, &composited);
        all_frames.push((label, composited, SCREEN_W, SCREEN_H));
    }
    println!("rendered {} toast frames", all_frames.len());

    // ── Menu-only frames (each cursor position) ─────────────
    let items = config.menu.items.clone();
    let menu_start = all_frames.len();

    for cursor in 0..items.len() {
        let mut menu = Menu::new(items.clone());
        force_menu_open(&mut menu, cursor);
        let state = FrameState {
            popup: None,
            menu: Some(&menu),
            menu_config: &config.menu,
            game_name: Some("Super Mario World"),
        };
        let argb = rend.render_frame(&state, SCREEN_W, SCREEN_H);
        let composited = composite_over_bg(&game_bg, &argb, SCREEN_W, SCREEN_H);
        let label = format!("menu-cursor-{}", cursor);
        save_argb_png(&out.join(format!("{}.png", label)), SCREEN_W, SCREEN_H, &composited);
        all_frames.push((label, composited, SCREEN_W, SCREEN_H));
    }

    // Confirm state
    {
        let mut menu = Menu::new(items.clone());
        let last = items.len().saturating_sub(1);
        force_menu_open(&mut menu, last);
        menu.select();
        if menu.is_visible() {
            let state = FrameState {
                popup: None,
                menu: Some(&menu),
                menu_config: &config.menu,
                game_name: Some("Super Mario World"),
            };
            let argb = rend.render_frame(&state, SCREEN_W, SCREEN_H);
            let composited = composite_over_bg(&game_bg, &argb, SCREEN_W, SCREEN_H);
            let label = "menu-confirm".to_string();
            save_argb_png(&out.join(format!("{}.png", label)), SCREEN_W, SCREEN_H, &composited);
            all_frames.push((label, composited, SCREEN_W, SCREEN_H));
        }
    }
    println!("rendered {} menu frames", all_frames.len() - menu_start);

    // ── Combined: menu + toast simultaneously ───────────────
    {
        let mut menu = Menu::new(items.clone());
        force_menu_open(&mut menu, 1);
        let mut popup = Popup::new("While In Menu".to_string(), "Achievement while menu is open".to_string());
        popup.force_hold();
        let state = FrameState {
            popup: Some(&popup),
            menu: Some(&menu),
            menu_config: &config.menu,
            game_name: Some("Chrono Trigger"),
        };
        let argb = rend.render_frame(&state, SCREEN_W, SCREEN_H);
        let composited = composite_over_bg(&game_bg, &argb, SCREEN_W, SCREEN_H);
        let label = "combined-menu-toast".to_string();
        save_argb_png(&out.join(format!("{}.png", label)), SCREEN_W, SCREEN_H, &composited);
        all_frames.push((label, composited, SCREEN_W, SCREEN_H));
    }

    // ── Build atlas ─────────────────────────────────────────
    let cols = 3_u32;
    let thumb_w = SCREEN_W / 2;
    let thumb_h = SCREEN_H / 2;
    let pad = 8_u32;
    let rows = ((all_frames.len() as u32) + cols - 1) / cols;

    let atlas_w = cols * (thumb_w + pad) + pad;
    let atlas_h = rows * (thumb_h + pad) + pad;

    let bg_r: u8 = 30;
    let bg_g: u8 = 30;
    let bg_b: u8 = 46;

    let mut atlas: Vec<u8> = vec![0; (atlas_w * atlas_h * 4) as usize];
    for i in 0..(atlas_w * atlas_h) as usize {
        atlas[i * 4] = bg_r;
        atlas[i * 4 + 1] = bg_g;
        atlas[i * 4 + 2] = bg_b;
        atlas[i * 4 + 3] = 255;
    }

    for (idx, (_label, argb, fw, fh)) in all_frames.iter().enumerate() {
        let col = idx as u32 % cols;
        let row = idx as u32 / cols;
        let ox = pad + col * (thumb_w + pad);
        let oy = pad + row * (thumb_h + pad);

        // Downsample 2x and blit
        for ty in 0..thumb_h {
            for tx in 0..thumb_w {
                let sx = (tx * 2).min(fw - 1);
                let sy = (ty * 2).min(fh - 1);
                let si = (sy * fw + sx) as usize;
                if si >= argb.len() { continue; }
                let pixel = argb[si];
                let sa = ((pixel >> 24) & 0xFF) as u16;
                if sa == 0 { continue; }
                let sr = ((pixel >> 16) & 0xFF) as u16;
                let sg = ((pixel >> 8) & 0xFF) as u16;
                let sb = (pixel & 0xFF) as u16;

                let dx = ox + tx;
                let dy = oy + ty;
                if dx >= atlas_w || dy >= atlas_h { continue; }
                let di = ((dy * atlas_w + dx) * 4) as usize;
                if di + 3 >= atlas.len() { continue; }

                let da = 255u16 - sa;
                atlas[di]     = ((sr * sa + atlas[di] as u16 * da) / 255) as u8;
                atlas[di + 1] = ((sg * sa + atlas[di + 1] as u16 * da) / 255) as u8;
                atlas[di + 2] = ((sb * sa + atlas[di + 2] as u16 * da) / 255) as u8;
                atlas[di + 3] = 255;
            }
        }
    }

    let atlas_path = out.join("atlas.png");
    save_rgba_png(&atlas_path, atlas_w, atlas_h, &atlas);
    println!("\natlas: {} ({}x{}, {} frames)", atlas_path.display(), atlas_w, atlas_h, all_frames.len());
    println!("individual frames in {}/", out.display());

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&atlas_path).spawn();
    }
}

fn force_menu_open(menu: &mut Menu, cursor: usize) {
    menu.toggle();
    std::thread::sleep(std::time::Duration::from_millis(250));
    menu.tick();
    for _ in 0..cursor {
        menu.move_down();
    }
}

/// Generate a simple colored placeholder PNG badge.
fn generate_placeholder_badge(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            let t = x as f32 / w as f32;
            let r = (255.0 * (1.0 - t * 0.3)) as u8;
            let g = (200.0 * (1.0 - t * 0.5)) as u8;
            let b = (50.0 + t * 50.0) as u8;
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(255);
        }
    }
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(std::io::Cursor::new(&mut buf), w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    buf
}

fn save_argb_png(path: &std::path::Path, w: u32, h: u32, argb: &[u32]) {
    let mut rgba = Vec::with_capacity(argb.len() * 4);
    for &pixel in argb {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(((pixel >> 24) & 0xFF) as u8);
    }
    save_rgba_png(path, w, h, &rgba);
}

fn save_rgba_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let buf = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(buf, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

/// Generate a fake game screenshot background (dark gradient with some color).
fn generate_game_background(w: u32, h: u32) -> Vec<u32> {
    let mut argb = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let tx = x as f32 / w as f32;
            let ty = y as f32 / h as f32;
            // Dark scene: deep blue-green gradient with some variation
            let r = (15.0 + 25.0 * ty + 10.0 * tx) as u8;
            let g = (20.0 + 35.0 * ty + 15.0 * (1.0 - tx)) as u8;
            let b = (40.0 + 30.0 * (1.0 - ty) + 20.0 * tx) as u8;
            argb.push(0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
        }
    }
    argb
}

/// Composite overlay (ARGB with alpha) over an opaque background.
fn composite_over_bg(bg: &[u32], overlay: &[u32], _w: u32, _h: u32) -> Vec<u32> {
    bg.iter()
        .zip(overlay.iter())
        .map(|(&bg_px, &ov_px)| {
            let oa = ((ov_px >> 24) & 0xFF) as u16;
            if oa == 0 {
                return bg_px;
            }
            if oa == 255 {
                return ov_px;
            }
            let inv = 255 - oa;
            let or = ((ov_px >> 16) & 0xFF) as u16;
            let og = ((ov_px >> 8) & 0xFF) as u16;
            let ob = (ov_px & 0xFF) as u16;
            let br = ((bg_px >> 16) & 0xFF) as u16;
            let bg_g = ((bg_px >> 8) & 0xFF) as u16;
            let bb = (bg_px & 0xFF) as u16;
            let r = (or * oa + br * inv) / 255;
            let g = (og * oa + bg_g * inv) / 255;
            let b = (ob * oa + bb * inv) / 255;
            0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
        })
        .collect()
}
