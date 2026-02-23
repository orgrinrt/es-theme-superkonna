//! superkonna-overlay: Themed achievement popup + ingame menu overlay for Batocera (X11)
//!
//! Monitors RetroArch's log file for RetroAchievements events, listens on a
//! Unix socket for menu commands, and renders themed popups and an ingame menu
//! using an X11 override-redirect window.

mod audio;
mod popup;
mod retroarch;
mod socket;
mod watcher;
mod window;

use superkonna_overlay::config;
use superkonna_overlay::menu;
use superkonna_overlay::renderer;
use superkonna_overlay::theme;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};

const SOCKET_PATH: &str = "/tmp/superkonna-overlay.sock";

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

    // Load overlay config (menu items, RetroArch connection, sounds)
    let overlay_config = config::OverlayConfig::find_and_load(&theme_root);
    info!("Menu config: {} items, title={}", overlay_config.menu.items.len(), overlay_config.menu.title);

    // Load theme colors and fonts
    let theme = match theme::Theme::load(&theme_root) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to load theme: {e}");
            std::process::exit(1);
        }
    };
    info!("Theme loaded: fg={} bg={} accent={}", theme.fg_color, theme.bg_color, theme.accent_color);

    // Create X11 window at full screen size (hidden initially)
    let init_w: u16 = std::env::var("SCREEN_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1920);
    let init_h: u16 = std::env::var("SCREEN_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1080);

    let mut win = match window::OverlayWindow::new(init_w, init_h) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create X11 window: {e}");
            std::process::exit(1);
        }
    };
    win.hide();
    let (screen_w, screen_h) = win.screen_size();
    info!("X11 overlay window created ({}x{})", screen_w, screen_h);

    // Create renderer
    let rend = renderer::Renderer::new(&theme);

    // Create RetroArch client
    let ra_client = retroarch::RetroArchClient::new(
        &overlay_config.menu.retroarch.host,
        overlay_config.menu.retroarch.port,
    )
    .ok();
    if ra_client.is_some() {
        info!("RetroArch UDP client ready");
    } else {
        warn!("Could not create RetroArch UDP client");
    }

    // Unified event channel
    enum Event {
        Achievement(watcher::AchievementEvent),
        Socket(socket::SocketCommand),
    }

    let (tx, rx) = mpsc::channel::<Event>();

    // Spawn log watcher thread with relay
    {
        let tx = tx.clone();
        let watcher_log_path = log_path.clone();
        std::thread::spawn(move || {
            let (atx, arx) = mpsc::channel();
            std::thread::spawn(move || {
                if let Err(e) = watcher::watch_log(&watcher_log_path, atx) {
                    error!("Log watcher error: {e}");
                }
            });
            for event in arx {
                if tx.send(Event::Achievement(event)).is_err() {
                    break;
                }
            }
        });
    }
    info!("Log watcher started");

    // Spawn socket listener thread with relay
    {
        let tx = tx.clone();
        let socket_path = SOCKET_PATH.to_string();
        std::thread::spawn(move || {
            let (stx, srx) = mpsc::channel();
            std::thread::spawn(move || {
                if let Err(e) = socket::listen(&socket_path, stx) {
                    error!("Socket listener error: {e}");
                }
            });
            for cmd in srx {
                if tx.send(Event::Socket(cmd)).is_err() {
                    break;
                }
            }
        });
    }
    info!("Socket listener started at {SOCKET_PATH}");

    // State
    let mut popup_queue = popup::PopupQueue::new();
    let mut game_menu = menu::Menu::new(overlay_config.menu.items.clone());
    let menu_config = overlay_config.menu.clone();
    let frame_duration = Duration::from_millis(16); // ~60fps

    // Sound paths
    let sounds_dir = theme_root.join("assets").join("sounds");

    loop {
        let frame_start = Instant::now();

        // Drain incoming events (non-blocking)
        while let Ok(event) = rx.try_recv() {
            debug!("Main loop received event");
            match event {
                Event::Achievement(ach) => {
                    info!("Achievement: {} — {}", ach.title, ach.description);
                    popup_queue.push(popup::Popup::new(ach.title, ach.description));
                    // Play achievement sound
                    audio::play_sound(&sounds_dir.join("achievement.wav"));
                }
                Event::Socket(cmd) => match cmd {
                    socket::SocketCommand::MenuToggle => {
                        game_menu.toggle();
                        if game_menu.is_visible() {
                            if let Some(snd) = &menu_config.sound_select {
                                audio::play_sound(&sounds_dir.join(snd));
                            }
                        }
                    }
                    socket::SocketCommand::MenuUp => {
                        game_menu.move_up();
                        if let Some(snd) = &menu_config.sound_scroll {
                            audio::play_sound(&sounds_dir.join(snd));
                        }
                    }
                    socket::SocketCommand::MenuDown => {
                        game_menu.move_down();
                        if let Some(snd) = &menu_config.sound_scroll {
                            audio::play_sound(&sounds_dir.join(snd));
                        }
                    }
                    socket::SocketCommand::MenuSelect => {
                        if let Some(action) = game_menu.select() {
                            if let Some(snd) = &menu_config.sound_select {
                                audio::play_sound(&sounds_dir.join(snd));
                            }
                            match action {
                                menu::MenuAction::Dismiss => {}
                                menu::MenuAction::RetroArch(cmd) => {
                                    if let Some(ref client) = ra_client {
                                        client.send_command(&cmd);
                                    }
                                }
                                menu::MenuAction::Shell(cmd) => {
                                    info!("Executing shell: {cmd}");
                                    let _ = std::process::Command::new("sh")
                                        .args(["-c", &cmd])
                                        .spawn();
                                }
                            }
                        }
                    }
                    socket::SocketCommand::MenuBack => {
                        game_menu.back();
                        if let Some(snd) = &menu_config.sound_back {
                            audio::play_sound(&sounds_dir.join(snd));
                        }
                    }
                    socket::SocketCommand::Popup { title, description } => {
                        info!("Popup received via socket: {title} | {description}");
                        popup_queue.push(popup::Popup::new(title, description));
                    }
                },
            }
        }

        // Tick animations
        popup_queue.tick();
        game_menu.tick();

        // Determine what to render
        let has_popup = popup_queue.current().is_some();
        let has_menu = game_menu.is_visible();

        if has_menu {
            let pixels = rend.render_menu(&game_menu, screen_w as u32, screen_h as u32, &menu_config);
            win.show();
            win.update_pixels(&pixels, screen_w, screen_h);
        } else if has_popup {
            let popup = popup_queue.current().unwrap();
            let popup_pixels = rend.render_popup(&popup.title, &popup.description, popup.opacity());
            let sw = screen_w as u32;
            let sh = screen_h as u32;
            let total = (sw * sh) as usize;
            let mut screen = vec![0u32; total];
            let pw: u32 = 640;
            let ph: u32 = 140;
            let offset_x = sw.saturating_sub(pw + 20);
            let offset_y = 20_u32;
            for row in 0..ph {
                for col in 0..pw {
                    let src_idx = (row * pw + col) as usize;
                    let dst_idx = ((offset_y + row) * sw + offset_x + col) as usize;
                    if dst_idx < total {
                        screen[dst_idx] = popup_pixels[src_idx];
                    }
                }
            }
            win.show();
            win.update_pixels(&screen, screen_w, screen_h);
        } else {
            win.hide();
        }

        // Process X11 events
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
    if let Ok(root) = std::env::var("SUPERKONNA_THEME_ROOT") {
        return PathBuf::from(root);
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("theme.xml").exists() {
                return d;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    PathBuf::from("/userdata/themes/es-theme-superkonna")
}
