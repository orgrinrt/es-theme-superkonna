//! Menu state machine with cursor navigation and confirm logic.

use crate::config::MenuItem;
use std::time::Instant;

const OPEN_DURATION_MS: u64 = 200;
const CLOSE_DURATION_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuState {
    Closed,
    Opening,
    Open,
    Confirming { item_idx: usize },
    Closing,
}

#[derive(Debug)]
pub enum MenuAction {
    Dismiss,
    RetroArch(String),
    Shell(String),
}

pub struct Menu {
    state: MenuState,
    cursor: usize,
    items: Vec<MenuItem>,
    transition_start: Instant,
    dirty: bool,
}

impl Menu {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Menu {
            state: MenuState::Closed,
            cursor: 0,
            items,
            transition_start: Instant::now(),
            dirty: false,
        }
    }

    pub fn toggle(&mut self) {
        match self.state {
            MenuState::Closed => {
                self.state = MenuState::Opening;
                self.transition_start = Instant::now();
                self.cursor = 0;
                self.dirty = true;
            }
            MenuState::Open | MenuState::Confirming { .. } => {
                self.state = MenuState::Closing;
                self.transition_start = Instant::now();
                self.dirty = true;
            }
            _ => {}
        }
    }

    pub fn move_up(&mut self) {
        if !matches!(self.state, MenuState::Open) {
            return;
        }
        if self.items.is_empty() {
            return;
        }
        self.cursor = if self.cursor == 0 {
            self.items.len() - 1
        } else {
            self.cursor - 1
        };
        self.dirty = true;
    }

    pub fn move_down(&mut self) {
        if !matches!(self.state, MenuState::Open) {
            return;
        }
        if self.items.is_empty() {
            return;
        }
        self.cursor = (self.cursor + 1) % self.items.len();
        self.dirty = true;
    }

    /// Attempt to select the current item. Returns the action to execute, if any.
    pub fn select(&mut self) -> Option<MenuAction> {
        if self.items.is_empty() {
            return None;
        }

        match self.state {
            MenuState::Open => {
                let item = &self.items[self.cursor];
                if item.confirm {
                    self.state = MenuState::Confirming { item_idx: self.cursor };
                    self.dirty = true;
                    return None;
                }
                self.execute_item(self.cursor)
            }
            MenuState::Confirming { item_idx } => self.execute_item(item_idx),
            _ => None,
        }
    }

    pub fn back(&mut self) {
        match self.state {
            MenuState::Open => {
                self.state = MenuState::Closing;
                self.transition_start = Instant::now();
                self.dirty = true;
            }
            MenuState::Confirming { .. } => {
                self.state = MenuState::Open;
                self.dirty = true;
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) {
        let elapsed = self.transition_start.elapsed().as_millis() as u64;
        match self.state {
            MenuState::Opening if elapsed >= OPEN_DURATION_MS => {
                self.state = MenuState::Open;
                self.dirty = true;
            }
            MenuState::Closing if elapsed >= CLOSE_DURATION_MS => {
                self.state = MenuState::Closed;
                self.dirty = true;
            }
            MenuState::Opening | MenuState::Closing => {
                self.dirty = true; // Still animating
            }
            _ => {}
        }
    }

    pub fn is_visible(&self) -> bool {
        !matches!(self.state, MenuState::Closed)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn state(&self) -> MenuState {
        self.state
    }

    /// Opacity for fade transitions (0.0 to 1.0).
    pub fn opacity(&self) -> f32 {
        let elapsed = self.transition_start.elapsed().as_millis() as f32;
        match self.state {
            MenuState::Closed => 0.0,
            MenuState::Opening => (elapsed / OPEN_DURATION_MS as f32).min(1.0),
            MenuState::Open | MenuState::Confirming { .. } => 1.0,
            MenuState::Closing => 1.0 - (elapsed / CLOSE_DURATION_MS as f32).min(1.0),
        }
    }

    /// Scale for open/close transition (0.95 to 1.0).
    pub fn scale(&self) -> f32 {
        0.95 + 0.05 * self.opacity()
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    fn execute_item(&mut self, idx: usize) -> Option<MenuAction> {
        let item = &self.items[idx];
        let action = match item.action.as_str() {
            "dismiss" => Some(MenuAction::Dismiss),
            "retroarch" => item.command.as_ref().map(|c| MenuAction::RetroArch(c.clone())),
            "shell" => item.command.as_ref().map(|c| MenuAction::Shell(c.clone())),
            _ => None,
        };

        // Close menu after action
        self.state = MenuState::Closing;
        self.transition_start = Instant::now();
        self.dirty = true;

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MenuItem;

    fn test_items() -> Vec<MenuItem> {
        vec![
            MenuItem { id: "resume".into(), label: "Resume".into(), icon: None, action: "dismiss".into(), command: None, confirm: false },
            MenuItem { id: "save".into(), label: "Save State".into(), icon: None, action: "retroarch".into(), command: Some("SAVE_STATE".into()), confirm: false },
            MenuItem { id: "quit".into(), label: "Quit".into(), icon: None, action: "retroarch".into(), command: Some("QUIT".into()), confirm: true },
        ]
    }

    #[test]
    fn toggle_opens_and_closes() {
        let mut menu = Menu::new(test_items());
        assert!(!menu.is_visible());

        menu.toggle();
        assert!(menu.is_visible());
        assert!(matches!(menu.state(), MenuState::Opening));

        // Simulate transition complete
        std::thread::sleep(std::time::Duration::from_millis(250));
        menu.tick();
        assert!(matches!(menu.state(), MenuState::Open));

        menu.toggle();
        assert!(matches!(menu.state(), MenuState::Closing));
    }

    #[test]
    fn cursor_wraps_around() {
        let mut menu = Menu::new(test_items());
        menu.state = MenuState::Open;

        assert_eq!(menu.cursor(), 0);
        menu.move_up();
        assert_eq!(menu.cursor(), 2); // Wrapped to last
        menu.move_down();
        assert_eq!(menu.cursor(), 0); // Wrapped to first
    }

    #[test]
    fn select_dismiss_returns_action() {
        let mut menu = Menu::new(test_items());
        menu.state = MenuState::Open;
        menu.cursor = 0;

        let action = menu.select();
        assert!(matches!(action, Some(MenuAction::Dismiss)));
    }

    #[test]
    fn select_retroarch_returns_command() {
        let mut menu = Menu::new(test_items());
        menu.state = MenuState::Open;
        menu.cursor = 1;

        let action = menu.select();
        assert!(matches!(action, Some(MenuAction::RetroArch(ref c)) if c == "SAVE_STATE"));
    }

    #[test]
    fn confirm_requires_double_select() {
        let mut menu = Menu::new(test_items());
        menu.state = MenuState::Open;
        menu.cursor = 2; // Quit (confirm = true)

        // First select enters confirming
        let action = menu.select();
        assert!(action.is_none());
        assert!(matches!(menu.state(), MenuState::Confirming { item_idx: 2 }));

        // Second select executes
        let action = menu.select();
        assert!(matches!(action, Some(MenuAction::RetroArch(ref c)) if c == "QUIT"));
    }

    #[test]
    fn back_cancels_confirm() {
        let mut menu = Menu::new(test_items());
        menu.state = MenuState::Open;
        menu.cursor = 2;

        menu.select(); // Enter confirming
        assert!(matches!(menu.state(), MenuState::Confirming { .. }));

        menu.back(); // Cancel
        assert!(matches!(menu.state(), MenuState::Open));
    }
}
