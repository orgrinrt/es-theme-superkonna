//! Unified input bindings — single source of truth.
//!
//! Reads `bindings.toml` (at theme root or override path) and provides:
//! - Semantic action definitions (button, gesture, hold timing)
//! - Menu item list with resolved bindings
//! - Query API for hint bar rendering and hold detection

use crate::buttons::Button;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use log::{info, warn};

// ── TOML schema ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BindingsFile {
    #[serde(default)]
    defaults: BindingDefaults,
    #[serde(default)]
    actions: HashMap<String, ActionDef>,
    #[serde(default)]
    menu: Vec<MenuItemDef>,
}

#[derive(Debug, Deserialize)]
struct BindingDefaults {
    #[serde(default = "default_hold_ms")]
    hold_ms: u64,
}

impl Default for BindingDefaults {
    fn default() -> Self {
        BindingDefaults { hold_ms: default_hold_ms() }
    }
}

#[derive(Debug, Deserialize)]
struct ActionDef {
    label: String,
    button: String,
    #[serde(default)]
    hold: bool,
    hold_ms: Option<u64>,
    #[serde(default)]
    confirm: bool,
}

#[derive(Debug, Deserialize)]
struct MenuItemDef {
    id: String,
    label: String,
    icon: Option<String>,
    action_type: String,
    command: Option<String>,
    bind_action: Option<String>,
}

fn default_hold_ms() -> u64 { 1500 }

// ── Public types ────────────────────────────────────────────

/// A resolved semantic action with its binding.
#[derive(Debug, Clone)]
pub struct Action {
    pub name: String,
    pub label: String,
    pub button: Button,
    pub button_name: String,
    pub hold: bool,
    pub hold_ms: u64,
    pub confirm: bool,
}

/// A menu item with its resolved binding (if any).
#[derive(Debug, Clone)]
pub struct BoundMenuItem {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub action_type: String,
    pub command: Option<String>,
    pub confirm: bool,
    /// Resolved binding from the action this item references.
    pub binding: Option<Action>,
}

/// The full resolved bindings config.
pub struct Bindings {
    pub actions: HashMap<String, Action>,
    pub menu_items: Vec<BoundMenuItem>,
}

impl Bindings {
    /// Load and resolve bindings from a TOML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let file: BindingsFile = toml::from_str(&content)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        Ok(Self::resolve(file))
    }

    /// Search for bindings.toml in standard locations.
    pub fn find_and_load(theme_root: &Path) -> Self {
        let candidates = vec![
            std::env::var("SUPERKONNA_BINDINGS").ok().map(std::path::PathBuf::from),
            Some(std::path::PathBuf::from("/userdata/system/superkonna-overlay/bindings.toml")),
            Some(theme_root.join("bindings.toml")),
        ];

        for path in candidates.into_iter().flatten() {
            if path.exists() {
                match Self::load(&path) {
                    Ok(b) => {
                        info!("Loaded bindings from {}", path.display());
                        return b;
                    }
                    Err(e) => warn!("Failed to load {}: {e}", path.display()),
                }
            }
        }

        info!("Using built-in default bindings");
        Self::builtin_default()
    }

    fn resolve(file: BindingsFile) -> Self {
        let default_hold = file.defaults.hold_ms;

        let actions: HashMap<String, Action> = file.actions.into_iter()
            .filter_map(|(name, def)| {
                let button = Button::from_name(&def.button)?;
                Some((name.clone(), Action {
                    name: name.clone(),
                    label: def.label,
                    button,
                    button_name: def.button,
                    hold: def.hold,
                    hold_ms: def.hold_ms.unwrap_or(default_hold),
                    confirm: def.confirm,
                }))
            })
            .collect();

        let menu_items: Vec<BoundMenuItem> = file.menu.into_iter()
            .map(|item| {
                let binding = item.bind_action.as_ref()
                    .and_then(|name| actions.get(name).cloned());
                let confirm = binding.as_ref().map(|b| b.confirm).unwrap_or(false);
                BoundMenuItem {
                    id: item.id,
                    label: item.label,
                    icon: item.icon,
                    action_type: item.action_type,
                    command: item.command,
                    confirm,
                    binding,
                }
            })
            .collect();

        Bindings { actions, menu_items }
    }

    fn builtin_default() -> Self {
        let toml_str = include_str!("../../../bindings.toml");
        let file: BindingsFile = toml::from_str(toml_str)
            .expect("built-in bindings.toml must be valid");
        Self::resolve(file)
    }

    /// All actions that have a press binding (not hold).
    pub fn press_actions(&self) -> Vec<&Action> {
        self.actions.values().filter(|a| !a.hold).collect()
    }

    /// All actions that have a hold binding.
    pub fn hold_actions(&self) -> Vec<&Action> {
        self.actions.values().filter(|a| a.hold).collect()
    }

    /// Get the action bound to a specific button press (non-hold).
    pub fn press_action_for(&self, button: Button) -> Option<&Action> {
        self.actions.values().find(|a| a.button == button && !a.hold)
    }

    /// Get the action bound to a specific button hold.
    pub fn hold_action_for(&self, button: Button) -> Option<&Action> {
        self.actions.values().find(|a| a.button == button && a.hold)
    }

    /// Get hints for the hint bar: (button, label, is_hold) tuples,
    /// ordered: press bindings first (confirm, back), then hold bindings.
    pub fn hint_bar_items(&self) -> Vec<HintBarItem> {
        let mut hints = Vec::new();

        // Always show confirm first
        if let Some(a) = self.actions.get("confirm") {
            hints.push(HintBarItem {
                button: a.button,
                label: a.label.clone(),
                hold: false,
                hold_ms: 0,
            });
        }

        // Then all menu-item bindings in menu order (skip confirm, it's already shown)
        for item in &self.menu_items {
            if let Some(ref binding) = item.binding {
                if binding.name == "confirm" { continue; }
                hints.push(HintBarItem {
                    button: binding.button,
                    label: binding.label.clone(),
                    hold: binding.hold,
                    hold_ms: binding.hold_ms,
                });
            }
        }

        hints
    }

    /// Convert to legacy MenuItem vec for the Menu state machine.
    pub fn to_menu_items(&self) -> Vec<crate::config::MenuItem> {
        self.menu_items.iter().map(|item| {
            let (bind, hold_bind, hold_ms) = match &item.binding {
                Some(b) if b.hold => (None, Some(b.button_name.clone()), b.hold_ms),
                Some(b) => (Some(b.button_name.clone()), None, 1500),
                None => (None, None, 1500),
            };
            crate::config::MenuItem {
                id: item.id.clone(),
                label: item.label.clone(),
                icon: item.icon.clone(),
                action: item.action_type.clone(),
                command: item.command.clone(),
                confirm: item.confirm,
                bind,
                hold_bind,
                hold_ms,
                hint_label: Some(item.binding.as_ref()
                    .map(|b| b.label.clone())
                    .unwrap_or_else(|| item.label.clone())),
            }
        }).collect()
    }
}

/// A single entry in the hint bar.
#[derive(Debug, Clone)]
pub struct HintBarItem {
    pub button: Button,
    pub label: String,
    pub hold: bool,
    pub hold_ms: u64,
}

impl HintBarItem {
    /// Return the button name as used in config (for hold_progress lookup).
    pub fn button_name_for_config(&self) -> String {
        match self.button {
            Button::A => "a",
            Button::B => "b",
            Button::X => "x",
            Button::Y => "y",
            Button::LB => "l1",
            Button::RB => "r1",
            Button::LT => "l2",
            Button::RT => "r2",
            Button::Start => "start",
            Button::Select => "select",
            Button::DpadUp => "up",
            Button::DpadDown => "down",
            Button::DpadLeft => "left",
            Button::DpadRight => "right",
        }.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_builtin_bindings() {
        let bindings = Bindings::builtin_default();
        assert!(bindings.actions.contains_key("confirm"));
        assert!(bindings.actions.contains_key("save_state"));
        assert!(bindings.actions.contains_key("quit_to_es"));

        let save = &bindings.actions["save_state"];
        assert!(save.hold);
        assert_eq!(save.button, Button::Y);
        assert_eq!(save.hold_ms, 1500);

        let quit = &bindings.actions["quit_to_es"];
        assert!(quit.hold);
        assert!(quit.confirm);
        assert_eq!(quit.button, Button::Start);
        assert_eq!(quit.hold_ms, 2000);
    }

    #[test]
    fn menu_items_have_bindings() {
        let bindings = Bindings::builtin_default();
        assert!(!bindings.menu_items.is_empty());

        let save_item = bindings.menu_items.iter()
            .find(|i| i.id == "save_state").unwrap();
        assert!(save_item.binding.is_some());
        let binding = save_item.binding.as_ref().unwrap();
        assert!(binding.hold);
        assert_eq!(binding.button, Button::Y);
    }

    #[test]
    fn hint_bar_has_confirm_first() {
        let bindings = Bindings::builtin_default();
        let hints = bindings.hint_bar_items();
        assert!(!hints.is_empty());
        assert_eq!(hints[0].label, "Select");
        assert!(!hints[0].hold);
    }

    #[test]
    fn to_legacy_menu_items() {
        let bindings = Bindings::builtin_default();
        let items = bindings.to_menu_items();
        assert!(!items.is_empty());

        let save = items.iter().find(|i| i.id == "save_state").unwrap();
        assert_eq!(save.hold_bind.as_deref(), Some("y"));
        assert!(save.bind.is_none());

        let resume = items.iter().find(|i| i.id == "resume").unwrap();
        assert_eq!(resume.bind.as_deref(), Some("b"));
        assert!(resume.hold_bind.is_none());
    }
}
