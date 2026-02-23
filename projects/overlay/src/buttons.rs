//! Controller button icon detection, mapping, and rasterization.
//!
//! Detects the connected controller style (Xbox, PlayStation, Switch, Steam Deck)
//! from ES input config, maps abstract button names to SVG icon paths, and
//! pre-rasterizes them at a given size for fast compositing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Controller style families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControllerStyle {
    Xbox,
    PlayStation,
    SteamDeck,
    Switch,
}

/// Abstract button names (SDL/ES convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    A,      // bottom face (confirm)
    B,      // right face (back)
    X,      // left face
    Y,      // top face
    LB,
    RB,
    LT,
    RT,
    Start,
    Select,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl Button {
    /// Parse from config bind string (case-insensitive).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "a" => Some(Button::A),
            "b" => Some(Button::B),
            "x" => Some(Button::X),
            "y" => Some(Button::Y),
            "lb" | "l1" => Some(Button::LB),
            "rb" | "r1" => Some(Button::RB),
            "lt" | "l2" => Some(Button::LT),
            "rt" | "r2" => Some(Button::RT),
            "start" => Some(Button::Start),
            "select" | "back" => Some(Button::Select),
            "dpad_up" | "up" => Some(Button::DpadUp),
            "dpad_down" | "down" => Some(Button::DpadDown),
            "dpad_left" | "left" => Some(Button::DpadLeft),
            "dpad_right" | "right" => Some(Button::DpadRight),
            _ => None,
        }
    }
}

/// Pre-rasterized button icon (RGBA pixels at a fixed size).
pub struct ButtonIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Cached set of button icons for the detected controller style.
pub struct ButtonIcons {
    pub style: ControllerStyle,
    icons: HashMap<Button, ButtonIcon>,
}

impl ButtonIcons {
    /// Detect controller and load button icons at the given pixel size.
    pub fn load(theme_root: &Path, icon_size: u32) -> Self {
        let style = detect_controller_style();
        let buttons_dir = theme_root.join("assets/buttons");

        let mut icons = HashMap::new();
        let buttons = [
            Button::A, Button::B, Button::X, Button::Y,
            Button::LB, Button::RB, Button::LT, Button::RT,
            Button::Start, Button::Select,
            Button::DpadUp, Button::DpadDown, Button::DpadLeft, Button::DpadRight,
        ];

        for btn in buttons {
            let rel = icon_file(btn, style);
            let path = buttons_dir.join(rel);
            if let Some(icon) = rasterize_svg(&path, icon_size) {
                icons.insert(btn, icon);
            }
        }

        log::info!("loaded {} button icons for {:?} at {}px", icons.len(), style, icon_size);
        ButtonIcons { style, icons }
    }

    /// Get a pre-rasterized icon for a button.
    pub fn get(&self, button: Button) -> Option<&ButtonIcon> {
        self.icons.get(&button)
    }
}

/// Detect controller style from ES input config.
fn detect_controller_style() -> ControllerStyle {
    // Check env override first
    if let Ok(val) = std::env::var("SUPERKONNA_CONTROLLER_STYLE") {
        match val.to_lowercase().as_str() {
            "playstation" | "ps" => return ControllerStyle::PlayStation,
            "switch" | "nintendo" => return ControllerStyle::Switch,
            "steamdeck" | "steam" => return ControllerStyle::SteamDeck,
            "xbox" => return ControllerStyle::Xbox,
            _ => {}
        }
    }

    let es_input = PathBuf::from("/userdata/system/configs/emulationstation/es_input.cfg");
    if !es_input.exists() {
        return ControllerStyle::Xbox;
    }

    let content = match std::fs::read_to_string(&es_input) {
        Ok(c) => c.to_lowercase(),
        Err(_) => return ControllerStyle::Xbox,
    };

    // Find last deviceName attribute (most recently connected)
    let name = content.lines().rev()
        .filter_map(|line| {
            let idx = line.find("devicename=\"")?;
            let start = idx + "devicename=\"".len();
            let end = line[start..].find('"')? + start;
            Some(line[start..end].to_string())
        })
        .next()
        .unwrap_or_default();

    if name.contains("playstation") || name.contains("dualshock")
        || name.contains("dualsense") || name.contains("sony") {
        ControllerStyle::PlayStation
    } else if name.contains("steam") && name.contains("deck")
        || name.contains("valve") {
        ControllerStyle::SteamDeck
    } else if name.contains("nintendo") || name.contains("switch")
        || name.contains("pro controller") || name.contains("joy-con")
        || name.contains("joycon") {
        ControllerStyle::Switch
    } else {
        ControllerStyle::Xbox
    }
}

/// Map abstract button to controller-specific SVG path (relative to assets/buttons/).
fn icon_file(button: Button, style: ControllerStyle) -> PathBuf {
    let s = match style {
        ControllerStyle::Xbox => match button {
            Button::A          => "xbox/xbox_button_a.svg",
            Button::B          => "xbox/xbox_button_b.svg",
            Button::X          => "xbox/xbox_button_x.svg",
            Button::Y          => "xbox/xbox_button_y.svg",
            Button::LB         => "xbox/xbox_lb.svg",
            Button::RB         => "xbox/xbox_rb.svg",
            Button::LT         => "xbox/xbox_lt.svg",
            Button::RT         => "xbox/xbox_rt.svg",
            Button::Start      => "xbox/xbox_button_menu.svg",
            Button::Select     => "xbox/xbox_button_view.svg",
            Button::DpadUp     => "xbox/xbox_dpad_up.svg",
            Button::DpadDown   => "xbox/xbox_dpad_down.svg",
            Button::DpadLeft   => "xbox/xbox_dpad_left.svg",
            Button::DpadRight  => "xbox/xbox_dpad_right.svg",
        },
        ControllerStyle::PlayStation => match button {
            Button::A          => "playstation/playstation_button_cross.svg",
            Button::B          => "playstation/playstation_button_circle.svg",
            Button::X          => "playstation/playstation_button_square.svg",
            Button::Y          => "playstation/playstation_button_triangle.svg",
            Button::LB         => "playstation/playstation_trigger_l1.svg",
            Button::RB         => "playstation/playstation_trigger_r1.svg",
            Button::LT         => "playstation/playstation_trigger_l2.svg",
            Button::RT         => "playstation/playstation_trigger_r2.svg",
            Button::Start      => "playstation/playstation5_button_options.svg",
            Button::Select     => "playstation/playstation5_button_create.svg",
            Button::DpadUp     => "playstation/playstation_dpad_up.svg",
            Button::DpadDown   => "playstation/playstation_dpad_down.svg",
            Button::DpadLeft   => "playstation/playstation_dpad_left.svg",
            Button::DpadRight  => "playstation/playstation_dpad_right.svg",
        },
        ControllerStyle::SteamDeck => match button {
            Button::A          => "steamdeck/steamdeck_button_a.svg",
            Button::B          => "steamdeck/steamdeck_button_b.svg",
            Button::X          => "steamdeck/steamdeck_button_x.svg",
            Button::Y          => "steamdeck/steamdeck_button_y.svg",
            Button::LB         => "steamdeck/steamdeck_button_l1.svg",
            Button::RB         => "steamdeck/steamdeck_button_r1.svg",
            Button::LT         => "steamdeck/steamdeck_button_l2.svg",
            Button::RT         => "steamdeck/steamdeck_button_r2.svg",
            Button::Start      => "steamdeck/steamdeck_button_options.svg",
            Button::Select     => "steamdeck/steamdeck_button_view.svg",
            Button::DpadUp     => "steamdeck/steamdeck_dpad_up.svg",
            Button::DpadDown   => "steamdeck/steamdeck_dpad_down.svg",
            Button::DpadLeft   => "steamdeck/steamdeck_dpad_left.svg",
            Button::DpadRight  => "steamdeck/steamdeck_dpad_right.svg",
        },
        ControllerStyle::Switch => match button {
            // Nintendo: ES "a" (bottom) → Switch B (bottom), ES "b" (right) → Switch A (right)
            Button::A          => "switch/switch_button_b.svg",
            Button::B          => "switch/switch_button_a.svg",
            Button::X          => "switch/switch_button_y.svg",
            Button::Y          => "switch/switch_button_x.svg",
            Button::LB         => "switch/switch_button_l.svg",
            Button::RB         => "switch/switch_button_r.svg",
            Button::LT         => "switch/switch_button_zl.svg",
            Button::RT         => "switch/switch_button_zr.svg",
            Button::Start      => "switch/switch_button_plus.svg",
            Button::Select     => "switch/switch_button_minus.svg",
            Button::DpadUp     => "switch/switch_dpad_up.svg",
            Button::DpadDown   => "switch/switch_dpad_down.svg",
            Button::DpadLeft   => "switch/switch_dpad_left.svg",
            Button::DpadRight  => "switch/switch_dpad_right.svg",
        },
    };
    PathBuf::from(s)
}

/// Rasterize an SVG file to RGBA pixels at the given square size.
fn rasterize_svg(path: &Path, size: u32) -> Option<ButtonIcon> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("button SVG not found: {} ({})", path.display(), e);
            return None;
        }
    };

    let opts = resvg::usvg::Options::default();
    let tree = match resvg::usvg::Tree::from_data(&data, &opts) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("failed to parse SVG {}: {}", path.display(), e);
            return None;
        }
    };

    let svg_size = tree.size();
    let sx = size as f32 / svg_size.width();
    let sy = size as f32 / svg_size.height();
    let scale = sx.min(sy);
    let dx = (size as f32 - svg_size.width() * scale) / 2.0;
    let dy = (size as f32 - svg_size.height() * scale) / 2.0;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale)
        .post_translate(dx, dy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(ButtonIcon {
        rgba: pixmap.data().to_vec(),
        width: size,
        height: size,
    })
}
