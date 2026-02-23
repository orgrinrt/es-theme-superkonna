//! Theme color and font loader.
//! Parses the ES theme's XML variables to extract colors and font paths.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Theme {
    pub fg_color: Color,
    pub bg_color: Color,
    pub accent_color: Color,
    pub on_accent_color: Color,
    pub sect_color: Color,
    pub card_color: Color,
    pub shadow_color: Color,
    pub subtle_color: Color,
    pub font_display_path: PathBuf,
    pub font_path: PathBuf,
    pub font_light_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

impl Color {
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let bytes = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Color { r: bytes.0, g: bytes.1, b: bytes.2, a: bytes.3 })
    }

    #[allow(dead_code)]
    pub fn as_argb_u32(&self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
}

impl Theme {
    pub fn load(theme_root: &Path) -> Result<Self, String> {
        // Parse variables.xml for font paths and base defaults
        let vars_path = theme_root.join("variables.xml");
        let vars = if vars_path.exists() {
            parse_variables(&std::fs::read_to_string(&vars_path).map_err(|e| e.to_string())?)
        } else {
            HashMap::new()
        };

        // Discover the active color scheme from ES settings, then load it
        let color_vars = load_active_color_scheme(theme_root);

        let get_color = |key: &str, default: &str| -> Color {
            color_vars
                .get(key)
                .or_else(|| vars.get(key))
                .and_then(|v| Color::from_hex(v))
                .unwrap_or_else(|| Color::from_hex(default).unwrap())
        };

        let resolve_font = |key: &str, default: &str| -> PathBuf {
            let rel = vars.get(key).map(|s| s.as_str()).unwrap_or(default);
            let rel = rel.strip_prefix("./").unwrap_or(rel);
            theme_root.join(rel)
        };

        Ok(Theme {
            fg_color: get_color("fgColor", "FFFFFFFF"),
            bg_color: get_color("bgColor", "1A1A2EFF"),
            accent_color: get_color("mainColor", "E94560FF"),
            on_accent_color: get_color("onMainColor", "FFFFFFFF"),
            sect_color: get_color("sectColor", "0F3460FF"),
            card_color: get_color("cardColor", "16213EFF"),
            shadow_color: get_color("shadowColor", "000000FF"),
            subtle_color: get_color("subtleColor", "FFFFFFFF"),
            font_display_path: resolve_font("fontDisplay", "assets/fonts/Inter/Inter-Bold.otf"),
            font_path: resolve_font("fontBody", "assets/fonts/Inter/Inter-Regular.otf"),
            font_light_path: resolve_font("fontLight", "assets/fonts/Inter/Inter-Light.otf"),
        })
    }
}

/// Resolve the active color scheme by reading ES settings.
///
/// Search order:
/// 1. `SUPERKONNA_COLOR_SCHEME` env var (e.g. "snes")
/// 2. ES settings file — look for subset key matching this theme's "Color scheme"
/// 3. Fall back to "dark"
///
/// Then load `{theme_root}/settings/colors/{scheme}/main.xml`.
fn load_active_color_scheme(theme_root: &Path) -> HashMap<String, String> {
    let scheme = resolve_color_scheme_name(theme_root);
    log::info!("overlay color scheme: {scheme}");

    let color_path = theme_root.join(format!("settings/colors/{scheme}/main.xml"));
    if color_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&color_path) {
            return parse_variables(&content);
        }
    }

    // Fallback: try dark
    let fallback = theme_root.join("settings/colors/dark/main.xml");
    if fallback.exists() {
        if let Ok(content) = std::fs::read_to_string(&fallback) {
            return parse_variables(&content);
        }
    }

    HashMap::new()
}

fn resolve_color_scheme_name(theme_root: &Path) -> String {
    // 1. Env var override
    if let Ok(val) = std::env::var("SUPERKONNA_COLOR_SCHEME") {
        if !val.is_empty() {
            return val;
        }
    }

    // 2. Read ES settings to find the active subset
    //    Batocera stores subset choices as:
    //      <string name="subset.<theme-dir-name>.<SubsetName>" value="<choice>" />
    //    The theme dir name is the folder under /userdata/themes/
    let theme_dir_name = theme_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("es-theme-superkonna");

    let es_settings_paths = [
        // Batocera standard
        Path::new("/userdata/system/configs/emulationstation/es_settings.cfg"),
        // Active profile override
        Path::new("/userdata/system/es_settings.cfg"),
    ];

    for settings_path in &es_settings_paths {
        if let Ok(content) = std::fs::read_to_string(settings_path) {
            if let Some(scheme) = parse_es_subset(&content, theme_dir_name, "Color scheme") {
                return scheme;
            }
        }
    }

    "dark".to_string()
}

/// Parse an es_settings.cfg XML for a specific subset value.
/// Looks for: `<string name="subset.<theme>.<subset_name>" value="<val>" />`
fn parse_es_subset(xml: &str, theme_name: &str, subset_name: &str) -> Option<String> {
    // The key format varies by ES version. Try common patterns:
    //   subset.<theme>.<SubsetName>
    //   ThemeSubset
    //   ThemeColorSet
    let patterns = [
        format!("subset.{}.{}", theme_name, subset_name),
        format!("subset.{}.colorScheme", theme_name),
        "ThemeColorSet".to_string(),
    ];

    for line in xml.lines() {
        let trimmed = line.trim();
        for pat in &patterns {
            if trimmed.contains(&format!("name=\"{}\"", pat)) {
                // Extract value="..."
                if let Some(val_start) = trimmed.find("value=\"") {
                    let rest = &trimmed[val_start + 7..];
                    if let Some(val_end) = rest.find('"') {
                        let val = &rest[..val_end];
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Simple XML variable parser — extracts `<name>value</name>` from `<variables>` blocks.
fn parse_variables(xml: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let mut in_variables = false;

    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<variables>") {
            in_variables = true;
            continue;
        }
        if trimmed.starts_with("</variables>") {
            in_variables = false;
            continue;
        }
        if !in_variables {
            continue;
        }
        // Match pattern: <name>value</name>
        if let Some(rest) = trimmed.strip_prefix('<') {
            if let Some(tag_end) = rest.find('>') {
                let tag = &rest[..tag_end];
                if tag.contains('/') || tag.contains(' ') {
                    continue;
                }
                let after = &rest[tag_end + 1..];
                let close = format!("</{tag}>");
                if let Some(val_end) = after.find(&close) {
                    let value = &after[..val_end];
                    vars.insert(tag.to_string(), value.to_string());
                }
            }
        }
    }
    vars
}
