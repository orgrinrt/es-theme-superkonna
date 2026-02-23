//! TOML configuration for the overlay menu.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use log::{info, warn};

#[derive(Debug, Deserialize)]
pub struct OverlayConfig {
    pub menu: MenuConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MenuConfig {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_backdrop_opacity")]
    pub backdrop_opacity: f32,
    #[serde(default = "default_item_height")]
    pub item_height: u16,
    #[serde(default = "default_padding")]
    pub padding: u16,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
    pub sound_scroll: Option<String>,
    pub sound_select: Option<String>,
    pub sound_back: Option<String>,
    #[serde(default)]
    pub retroarch: RetroArchConfig,
    #[serde(default = "default_items")]
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetroArchConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for RetroArchConfig {
    fn default() -> Self {
        RetroArchConfig {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub action: String,
    pub command: Option<String>,
    #[serde(default)]
    pub confirm: bool,
    /// Quick-press shortcut button (e.g. "b" for Resume).
    pub bind: Option<String>,
    /// Hold-for-duration shortcut button (e.g. "y" for Save State).
    pub hold_bind: Option<String>,
    /// Hold duration in ms (default 1500).
    #[serde(default = "default_hold_ms")]
    pub hold_ms: u64,
    /// Short label shown in hint bar (defaults to label if absent).
    pub hint_label: Option<String>,
}

fn default_hold_ms() -> u64 { 1500 }

impl OverlayConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        toml::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))
    }

    /// Load config with fallback chain:
    /// 1. $SUPERKONNA_MENU_CONFIG env var
    /// 2. /userdata/system/superkonna-overlay/menu.toml
    /// 3. {theme_root}/projects/overlay/menu.toml
    /// 4. {theme_root}/menu.toml
    /// 5. Built-in defaults
    pub fn find_and_load(theme_root: &Path) -> Self {
        let candidates: Vec<PathBuf> = vec![
            std::env::var("SUPERKONNA_MENU_CONFIG").ok().map(PathBuf::from),
            Some(PathBuf::from("/userdata/system/superkonna-overlay/menu.toml")),
            Some(theme_root.join("projects/overlay/menu.toml")),
            Some(theme_root.join("menu.toml")),
        ]
        .into_iter()
        .flatten()
        .collect();

        for path in &candidates {
            if path.exists() {
                match Self::load(path) {
                    Ok(config) => {
                        info!("Loaded menu config from {}", path.display());
                        return config;
                    }
                    Err(e) => warn!("Failed to load {}: {e}", path.display()),
                }
            }
        }

        info!("Using built-in default menu config");
        Self::builtin_default()
    }

    fn builtin_default() -> Self {
        OverlayConfig {
            menu: MenuConfig {
                title: default_title(),
                width: default_width(),
                backdrop_opacity: default_backdrop_opacity(),
                item_height: default_item_height(),
                padding: default_padding(),
                corner_radius: default_corner_radius(),
                sound_scroll: Some("scroll.wav".into()),
                sound_select: Some("confirm.wav".into()),
                sound_back: Some("back.wav".into()),
                retroarch: RetroArchConfig::default(),
                items: default_items(),
            },
        }
    }
}

fn default_title() -> String { "GAME MENU".into() }
fn default_width() -> u16 { 420 }
fn default_backdrop_opacity() -> f32 { 0.6 }
fn default_item_height() -> u16 { 56 }
fn default_padding() -> u16 { 16 }
fn default_corner_radius() -> f32 { 16.0 }
fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 55355 }

fn default_items() -> Vec<MenuItem> {
    vec![
        MenuItem {
            id: "resume".into(), label: "Resume".into(), icon: Some("gamepad.svg".into()),
            action: "dismiss".into(), command: None, confirm: false,
            bind: Some("b".into()), hold_bind: None, hold_ms: default_hold_ms(),
            hint_label: None,
        },
        MenuItem {
            id: "save_state".into(), label: "Save State".into(), icon: Some("savestate.svg".into()),
            action: "retroarch".into(), command: Some("SAVE_STATE".into()), confirm: false,
            bind: None, hold_bind: Some("y".into()), hold_ms: 1500,
            hint_label: Some("Save".into()),
        },
        MenuItem {
            id: "load_state".into(), label: "Load State".into(), icon: Some("savestate.svg".into()),
            action: "retroarch".into(), command: Some("LOAD_STATE".into()), confirm: false,
            bind: None, hold_bind: Some("x".into()), hold_ms: 1500,
            hint_label: Some("Load".into()),
        },
        MenuItem {
            id: "quit_to_es".into(), label: "Quit to EmulationStation".into(), icon: Some("exit-to-app.svg".into()),
            action: "retroarch".into(), command: Some("QUIT".into()), confirm: true,
            bind: None, hold_bind: Some("start".into()), hold_ms: 2000,
            hint_label: Some("Quit".into()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_menu_toml() {
        let toml_str = r#"
[menu]
title = "TEST MENU"
width = 400
backdrop_opacity = 0.5
item_height = 48
padding = 12
corner_radius = 12.0

[menu.retroarch]
host = "127.0.0.1"
port = 55355

[[menu.items]]
id = "resume"
label = "Resume"
action = "dismiss"

[[menu.items]]
id = "quit"
label = "Quit"
action = "retroarch"
command = "QUIT"
confirm = true
"#;
        let config: OverlayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.menu.title, "TEST MENU");
        assert_eq!(config.menu.items.len(), 2);
        assert_eq!(config.menu.items[0].action, "dismiss");
        assert!(config.menu.items[1].confirm);
        assert_eq!(config.menu.items[1].command.as_deref(), Some("QUIT"));
    }

    #[test]
    fn builtin_default_has_four_items() {
        let config = OverlayConfig::builtin_default();
        assert_eq!(config.menu.items.len(), 4);
        assert_eq!(config.menu.items[0].id, "resume");
        assert_eq!(config.menu.items[3].id, "quit_to_es");
    }
}
