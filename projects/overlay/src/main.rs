//! superkonna-overlay: Themed achievement popup overlay for Batocera (X11)
//!
//! Monitors RetroArch's log file for RetroAchievements events and displays
//! themed popup notifications using an X11 override-redirect window.

mod popup;
mod renderer;
mod theme;
mod watcher;
mod window;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use log::{error, info};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Resolve paths
    let theme_root = find_theme_root();
    info!("Theme root: {}", theme_root.display());

    let log_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/retroarch.log"));
    info!("Watching log: {}", log_path.display());

    // Load theme colors and fonts
    let theme = match theme::Theme::load(&theme_root) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to load theme: {e}");
            std::process::exit(1);
        }
    };
    info!("Theme loaded: fg={} bg={} accent={}", theme.fg_color, theme.bg_color, theme.accent_color);

    // Create X11 window
    let mut win = match window::OverlayWindow::new(480, 120) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create X11 window: {e}");
            std::process::exit(1);
        }
    };
    info!("X11 overlay window created");

    // Channel for achievement events from log watcher
    let (tx, rx) = mpsc::channel::<watcher::AchievementEvent>();

    // Spawn log watcher thread
    let watcher_log_path = log_path.clone();
    std::thread::spawn(move || {
        if let Err(e) = watcher::watch_log(&watcher_log_path, tx) {
            error!("Log watcher error: {e}");
        }
    });
    info!("Log watcher started");

    // Create renderer
    let rend = renderer::Renderer::new(&theme, 480, 120);

    // Main event loop
    let mut queue = popup::PopupQueue::new();
    let frame_duration = Duration::from_millis(16); // ~60fps

    loop {
        let frame_start = Instant::now();

        // Drain incoming events
        while let Ok(event) = rx.try_recv() {
            info!("Achievement: {} — {}", event.title, event.description);
            queue.push(popup::Popup::new(event.title, event.description));
        }

        // Tick animation state
        queue.tick();

        // Render if there's an active popup
        if let Some(popup) = queue.current() {
            let pixels = rend.render_popup(&popup.title, &popup.description, popup.opacity());
            win.show();
            win.update_pixels(&pixels, 480, 120);
        } else {
            win.hide();
        }

        // Process X11 events (non-blocking)
        win.poll_events();

        // Frame timing
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}

/// Walk up from the binary's location to find the theme root (has theme.xml).
fn find_theme_root() -> PathBuf {
    // Try environment variable first
    if let Ok(root) = std::env::var("SUPERKONNA_THEME_ROOT") {
        return PathBuf::from(root);
    }

    // Try relative to executable
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("theme.xml").exists() {
                return d;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Fallback: Batocera default theme path
    PathBuf::from("/userdata/themes/es-theme-superkonna")
}
