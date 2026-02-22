//! Popup queue and animation timing.
//! Manages a queue of achievement popups with slide-in, hold, and fade-out phases.

use std::time::{Duration, Instant};

const SLIDE_IN_MS: u64 = 300;
const HOLD_MS: u64 = 4000;
const FADE_OUT_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    SlideIn,
    Hold,
    FadeOut,
    Done,
}

#[derive(Debug)]
pub struct Popup {
    pub title: String,
    pub description: String,
    started: Instant,
    phase: Phase,
}

impl Popup {
    pub fn new(title: String, description: String) -> Self {
        Popup {
            title,
            description,
            started: Instant::now(),
            phase: Phase::SlideIn,
        }
    }

    /// Current opacity (0.0 to 1.0) based on animation phase.
    pub fn opacity(&self) -> f32 {
        let elapsed = self.started.elapsed().as_millis() as u64;
        match self.phase {
            Phase::SlideIn => {
                (elapsed as f32 / SLIDE_IN_MS as f32).min(1.0)
            }
            Phase::Hold => 1.0,
            Phase::FadeOut => {
                let fade_elapsed = elapsed.saturating_sub(SLIDE_IN_MS + HOLD_MS);
                1.0 - (fade_elapsed as f32 / FADE_OUT_MS as f32).min(1.0)
            }
            Phase::Done => 0.0,
        }
    }

    /// Horizontal slide offset (0.0 = fully visible, 1.0 = off-screen right).
    pub fn slide_offset(&self) -> f32 {
        let elapsed = self.started.elapsed().as_millis() as u64;
        match self.phase {
            Phase::SlideIn => {
                let t = (elapsed as f32 / SLIDE_IN_MS as f32).min(1.0);
                // Ease-out: decelerate into position
                1.0 - (1.0 - (1.0 - t).powi(3))
            }
            _ => 0.0,
        }
    }

    fn tick(&mut self) {
        let elapsed = self.started.elapsed().as_millis() as u64;
        self.phase = if elapsed < SLIDE_IN_MS {
            Phase::SlideIn
        } else if elapsed < SLIDE_IN_MS + HOLD_MS {
            Phase::Hold
        } else if elapsed < SLIDE_IN_MS + HOLD_MS + FADE_OUT_MS {
            Phase::FadeOut
        } else {
            Phase::Done
        };
    }

    fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
}

#[derive(Debug)]
pub struct PopupQueue {
    queue: Vec<Popup>,
}

impl PopupQueue {
    pub fn new() -> Self {
        PopupQueue { queue: Vec::new() }
    }

    pub fn push(&mut self, popup: Popup) {
        self.queue.push(popup);
    }

    /// Advance animation state, remove finished popups.
    pub fn tick(&mut self) {
        if let Some(popup) = self.queue.first_mut() {
            popup.tick();
            if popup.is_done() {
                self.queue.remove(0);
                // Start the next popup immediately
                if let Some(next) = self.queue.first_mut() {
                    next.started = std::time::Instant::now();
                }
            }
        }
    }

    /// Get the currently displaying popup, if any.
    pub fn current(&self) -> Option<&Popup> {
        self.queue.first()
    }
}
