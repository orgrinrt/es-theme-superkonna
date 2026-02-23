//! Local preview tool — renders overlay popups and menus to PNG files + atlas.
//! No X11 needed; runs on macOS/Linux/Windows.
//!
//! Usage: cargo run --bin preview [-- --theme-root PATH]
//!
//! Outputs:
//!   preview-output/popup-*.png    — individual popup frames
//!   preview-output/menu-*.png     — individual menu frames
//!   preview-output/atlas.png      — single tiled overview image

use std::path::PathBuf;

use superkonna_overlay::config::OverlayConfig;
use superkonna_overlay::menu::Menu;
use superkonna_overlay::renderer::Renderer;
use superkonna_overlay::theme::Theme;

const POPUP_W: u32 = 640;
const POPUP_H: u32 = 140;
const MENU_W: u32 = 480;
const MENU_H: u32 = 400;

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

    // ── Popups ──────────────────────────────────────────────
    let popup_cases: Vec<(&str, &str)> = vec![
        ("First Blood", "Defeat the first enemy"),
        ("Speed Demon", "Complete level 1 in under 60 seconds"),
        ("Completionist", "Collect all 120 stars across all worlds"),
        ("Dragon Slayer Supreme", "A very long description to test truncation: defeat every dragon across every realm and dimension"),
        ("🏆 Platinum", "Earn all other achievements"),
    ];

    let mut popup_frames: Vec<(Vec<u32>, u32, u32)> = Vec::new();

    for (i, (title, desc)) in popup_cases.iter().enumerate() {
        // Render at full opacity for the atlas
        let argb = rend.render_popup(title, desc, 1.0);
        save_argb_png(&out.join(format!("popup-{}.png", i)), POPUP_W, POPUP_H, &argb);
        popup_frames.push((argb, POPUP_W, POPUP_H));
    }

    // Also render opacity ramp for first popup
    for (j, &op) in [0.15_f32, 0.4, 0.7, 1.0].iter().enumerate() {
        let argb = rend.render_popup("Fade Test", "Opacity ramp preview", op);
        save_argb_png(&out.join(format!("popup-fade-{}.png", j)), POPUP_W, POPUP_H, &argb);
        popup_frames.push((argb, POPUP_W, POPUP_H));
    }
    println!("rendered {} popup variants", popup_frames.len());

    // ── Menus ───────────────────────────────────────────────
    let mut menu_frames: Vec<(Vec<u32>, u32, u32)> = Vec::new();
    let items = config.menu.items.clone();

    // Each cursor position
    for cursor in 0..items.len() {
        let mut menu = Menu::new(items.clone());
        force_menu_open(&mut menu, cursor);
        let argb = rend.render_menu(&menu, MENU_W, MENU_H, &config.menu);
        save_argb_png(&out.join(format!("menu-cursor-{}.png", cursor)), MENU_W, MENU_H, &argb);
        menu_frames.push((argb, MENU_W, MENU_H));
    }

    // Confirm state on last item (if it has confirm)
    {
        let mut menu = Menu::new(items.clone());
        let last = items.len().saturating_sub(1);
        force_menu_open(&mut menu, last);
        menu.select(); // -> Confirming if confirm=true, else closes
        if menu.is_visible() {
            let argb = rend.render_menu(&menu, MENU_W, MENU_H, &config.menu);
            save_argb_png(&out.join("menu-confirm.png"), MENU_W, MENU_H, &argb);
            menu_frames.push((argb, MENU_W, MENU_H));
        }
    }
    println!("rendered {} menu variants", menu_frames.len());

    // ── Atlas ───────────────────────────────────────────────
    let label_h: u32 = 24;
    let pad: u32 = 16;
    let section_gap: u32 = 32;

    // Section 1: popups stacked vertically
    let popup_section_w = POPUP_W;
    let popup_section_h = popup_frames.len() as u32 * (POPUP_H + label_h + pad);

    // Section 2: menus in a row
    let menu_section_w = menu_frames.len() as u32 * (MENU_W + pad);
    let menu_section_h = MENU_H + label_h;

    let atlas_w = popup_section_w.max(menu_section_w) + pad * 2;
    let atlas_h = pad + popup_section_h + section_gap + menu_section_h + pad;

    let bg_r: u8 = 30;
    let bg_g: u8 = 30;
    let bg_b: u8 = 46; // Catppuccin base

    let mut atlas: Vec<u8> = vec![0; (atlas_w * atlas_h * 4) as usize];

    // Fill background
    for i in 0..(atlas_w * atlas_h) as usize {
        atlas[i * 4] = bg_r;
        atlas[i * 4 + 1] = bg_g;
        atlas[i * 4 + 2] = bg_b;
        atlas[i * 4 + 3] = 255;
    }

    // Blit popups
    let mut y_off = pad;
    for (_idx, (argb, pw, ph)) in popup_frames.iter().enumerate() {
        let x_off = pad;
        // Label area (just leave as bg, the popup itself is the visual)
        y_off += label_h;
        blit_argb_to_rgba(&mut atlas, atlas_w, x_off, y_off, argb, *pw, *ph);
        y_off += ph + pad;
    }

    // Blit menus in a row
    let menu_y = pad + popup_section_h + section_gap + label_h;
    for (idx, (argb, mw, mh)) in menu_frames.iter().enumerate() {
        let x_off = pad + idx as u32 * (MENU_W + pad);
        blit_argb_to_rgba(&mut atlas, atlas_w, x_off, menu_y, argb, *mw, *mh);
    }

    // Save atlas
    let atlas_path = out.join("atlas.png");
    save_rgba_png(&atlas_path, atlas_w, atlas_h, &atlas);
    println!("\natlas: {} ({}x{})", atlas_path.display(), atlas_w, atlas_h);
    println!("individual frames in {}/", out.display());

    // Try to open on macOS
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&atlas_path).spawn();
    }
}

/// Force a Menu into Open state at a given cursor position.
fn force_menu_open(menu: &mut Menu, cursor: usize) {
    menu.toggle(); // Closed -> Opening
    std::thread::sleep(std::time::Duration::from_millis(250));
    menu.tick(); // Opening -> Open
    for _ in 0..cursor {
        menu.move_down();
    }
}

/// Blit ARGB u32 buffer onto RGBA u8 atlas with alpha compositing.
fn blit_argb_to_rgba(
    dst: &mut [u8], dst_w: u32,
    ox: u32, oy: u32,
    src: &[u32], sw: u32, sh: u32,
) {
    for row in 0..sh {
        for col in 0..sw {
            let si = (row * sw + col) as usize;
            if si >= src.len() { continue; }
            let pixel = src[si];
            let sa = ((pixel >> 24) & 0xFF) as u16;
            if sa == 0 { continue; }
            let sr = ((pixel >> 16) & 0xFF) as u16;
            let sg = ((pixel >> 8) & 0xFF) as u16;
            let sb = (pixel & 0xFF) as u16;

            let dx = ox + col;
            let dy = oy + row;
            if dx >= dst_w { continue; }
            let di = ((dy * dst_w + dx) * 4) as usize;
            if di + 3 >= dst.len() { continue; }

            // Alpha composite: src over dst
            let da = 255u16 - sa;
            dst[di]     = ((sr * sa + dst[di] as u16 * da) / 255) as u8;
            dst[di + 1] = ((sg * sa + dst[di + 1] as u16 * da) / 255) as u8;
            dst[di + 2] = ((sb * sa + dst[di + 2] as u16 * da) / 255) as u8;
            dst[di + 3] = 255;
        }
    }
}

/// Save ARGB u32 buffer as PNG.
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

/// Save raw RGBA u8 buffer as PNG.
fn save_rgba_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let buf = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(buf, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}
