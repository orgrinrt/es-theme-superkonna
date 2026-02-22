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
        // Parse variables.xml for font paths
        let vars_path = theme_root.join("variables.xml");
        let vars = if vars_path.exists() {
            parse_variables(&std::fs::read_to_string(&vars_path).map_err(|e| e.to_string())?)
        } else {
            HashMap::new()
        };

        // Parse the active color palette — try settings/colorScheme/main.xml
        let color_path = theme_root.join("settings/colorScheme/main.xml");
        let color_vars = if color_path.exists() {
            parse_variables(&std::fs::read_to_string(&color_path).map_err(|e| e.to_string())?)
        } else {
            HashMap::new()
        };

        let get_color = |key: &str, default: &str| -> Color {
            color_vars
                .get(key)
                .or_else(|| vars.get(key))
                .and_then(|v| Color::from_hex(v))
                .unwrap_or_else(|| Color::from_hex(default).unwrap())
        };

        let resolve_font = |key: &str, default: &str| -> PathBuf {
            let rel = vars.get(key).map(|s| s.as_str()).unwrap_or(default);
            // Strip leading ./ for joining
            let rel = rel.strip_prefix("./").unwrap_or(rel);
            theme_root.join(rel)
        };

        Ok(Theme {
            fg_color: get_color("fgColor", "FFFFFFFF"),
            bg_color: get_color("bgColor", "1A1A2EFF"),
            accent_color: get_color("mainColor", "E94560FF"),
            on_accent_color: get_color("onMainColor", "FFFFFFFF"),
            card_color: get_color("cardColor", "16213EFF"),
            shadow_color: get_color("shadowColor", "000000FF"),
            subtle_color: get_color("subtleColor", "FFFFFFFF"),
            font_display_path: resolve_font("fontDisplay", "assets/fonts/Inter/Inter-Bold.otf"),
            font_path: resolve_font("fontBody", "assets/fonts/Inter/Inter-Regular.otf"),
            font_light_path: resolve_font("fontLight", "assets/fonts/Inter/Inter-Light.otf"),
        })
    }
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
