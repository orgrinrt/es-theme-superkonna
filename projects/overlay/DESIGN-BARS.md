# Superkonna Frontend Architecture

## The Shift: Romhoard replaces ES as primary frontend

ES gives us: emulator launching (actually `batocera-run`), gamelist browsing (romhoard
does better), controller nav (gamepad.js), scraping (romhoard does IGDB/TMDB), system
settings (Batocera web manager on :80), theming (XML hell).

ES costs us: XML theme limitations, bar conflicts, letterboxing hacks, settings
overrides, kiosk-as-fake-game workaround, no deep integration possible.

Nothing ES provides is hard to replicate. Romhoard + overlay + batocera-run covers it.

### New stack

```
┌─────────────────────────────────────────────────────────┐
│                    gamescope compositor                  │
│  ┌───────────────────────────────────────────────────┐  │
│  │ z=0  Romhoard kiosk (Chromium fullscreen)         │  │
│  │      Primary frontend: browse, detail, queue,     │  │
│  │      library, settings link                       │  │
│  ├───────────────────────────────────────────────────┤  │
│  │ z=0  batocera-run / RetroArch (when game running) │  │
│  ├───────────────────────────────────────────────────┤  │
│  │ z=2  superkonna-overlay (GAMESCOPE_EXTERNAL_OVERLAY) │
│  │      Persistent bars (top/bottom)                 │  │
│  │      Achievement toasts                           │  │
│  │      In-game quick menu                           │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

- **Gamescope** wraps everything: `-W 1920 -H 1080 -w 1920 -h 960`
  Inner apps see 1920x960. Overlay sees full 1920x1080.
- **Romhoard kiosk** autostarts instead of ES. Full-screen Chromium in kiosk mode.
- **Overlay** renders persistent chrome at compositor level.
- **ES** stays installed as fallback, launchable from romhoard settings page.

---

## Gamescope Integration

### Why gamescope

Gamescope is Valve's micro-compositor for SteamOS/Steam Deck. It wraps an X11/Wayland
session and provides:

- **Native letterboxing**: inner resolution (`-w`/`-h`) vs output resolution (`-W`/`-H`)
- **External overlay layer**: any X11 window tagged `GAMESCOPE_EXTERNAL_OVERLAY` composites
  at z=2, above the game, below cursor. No input passthrough hacks needed.
- **FSR/NIS upscaling**, adaptive sync, frame limiting — free bonuses.

### Batocera status

Gamescope works on Batocera (tested v42, nvidia). A draft PR exists
(batocera-linux/batocera.linux#13690) but is held for scope expansion (Wine-only → all
emulators). Maintainers want it for v43.

We build gamescope ourselves for our Batocera image. Dependencies are standard
(Vulkan, wayland, libseat — all available). The libseat build needs an explicit backend
flag; use pre-built .so or patch the build config.

### Launch sequence

```bash
# In autostart (replaces ES launch):
gamescope -f -W $SCREEN_W -H $SCREEN_H -w $SCREEN_W -h $VIEWPORT_H -- \
    superkonna-session
```

`superkonna-session` is a script that:
1. Starts `superkonna-overlay` in background (registers as external overlay)
2. Starts `romhoard` server (if not already running)
3. Launches Chromium in kiosk mode pointed at `http://localhost:1337/kiosk`
4. Waits for Chromium to exit (shouldn't normally happen)

### Overlay registration

```rust
// In window.rs, after creating the X11 window:
let atom = intern_atom(&conn, "GAMESCOPE_EXTERNAL_OVERLAY")?;
conn.change_property32(
    PropMode::REPLACE, window, atom, AtomEnum::CARDINAL, &[1]
)?;
```

Single full-screen window. Gamescope handles z-ordering and input routing.
No two-window hack needed. No shape extension needed.

---

## Bar Geometry

### Constants (at 1080p reference)

```
SCREEN_W            = 1920
SCREEN_H            = 1080
BAR_H               = 60        // each bar height
BAR_MARGIN_X        = 16        // inset from screen edges
BAR_MARGIN_Y        = 10        // inset from screen top/bottom edge
BAR_INNER_H         = 40        // BAR_H - BAR_MARGIN_Y * 2
BAR_RADIUS          = 14        // rounded corners
VIEWPORT_Y          = BAR_H     // 60   — where inner content starts
VIEWPORT_H          = 960       // SCREEN_H - BAR_H * 2
```

Scale proportionally at other resolutions: `bar_h = screen_h * 60 / 1080`.

### Visual style

Floating rounded pill panels (same aesthetic as the quick menu):
- Background: `card_color` at 75% opacity
- Subtle drop shadow (reuse existing `SHADOW_LAYERS`)
- Content vertically centered within bar
- Consistent with the in-game menu panel style

---

## Top Bar Content

```
┌────────────────────────────────────────────────────────────────┐
│  [avatar] username  K42kp  │  [controller dots]  │  🏆 ra_user │
│                            │                     │  [☀ 12°]    │
│                            │                     │  [🔋] 14:35  │
└────────────────────────────────────────────────────────────────┘
```

### Left cluster — Profile

| Element | Source | Update |
|---------|--------|--------|
| Avatar | `./generated/avatar.png` or romhoard API | File watch |
| Username | Socket `bar:username` or config | On login |
| Currency balance | Socket `bar:balance K42` | On transaction |

### Center cluster — Status

| Element | Source | Update |
|---------|--------|--------|
| Controller dots | `/dev/input` evdev | 500ms poll |
| Network status | `/sys/class/net/` or `nmcli` | 30s |

### Right cluster — System

| Element | Source | Update |
|---------|--------|--------|
| RA user | Config or socket | On login |
| Weather | `./generated/weather-*.svg` | File watch |
| Battery | `/sys/class/power_supply/` | 30s |
| Clock | System time | 1s |

---

## Bottom Bar Content

Context-sensitive controller hints:

```
┌────────────────────────────────────────────────────────────────┐
│  [A] select   [B] back        [Y] queue       [LB] ◀  [RB] ▶ │
└────────────────────────────────────────────────────────────────┘
```

### Contexts

| Context | Trigger | Hint actions |
|---------|---------|------|
| Browse | `bar:context browse` | confirm(details) back(settings) queue navigate |
| Game detail | `bar:context detail` | confirm(queue/launch) back page_prev(◀) page_next(▶) |
| Queue/downloads | `bar:context queue` | confirm(cancel/retry) back |
| Library | `bar:context library` | launch back options favorite |
| In-game | `bar:context ingame` | Bars hidden (overlay quick menu takes over) |
| Settings | `bar:context settings` | confirm(select) back |
| ES fallback | `bar:context es-system` | confirm(select) back(settings) favorite page_prev page_next |
| ES gamelist | `bar:context es-gamelist` | launch back options favorite |

Parenthesized text = label override for that context. Actions without overrides
use the default label from `bindings.toml`.

---

## Socket Protocol

Same Unix socket: `/tmp/superkonna-overlay.sock`. New `bar:` command prefix.

```
bar:context <name>              Set hint context (loads default hints from config)
bar:hints <json>                Dynamic semantic hints (see below)
bar:balance <text>              Update currency display
bar:username <text>             Update username
bar:avatar <path>               Load avatar image
bar:weather <icon-path> <temp>  Update weather
bar:hide                        Hide bars (video playback, etc.)
bar:show                        Show bars
bar:notify <title> <body>       General notification toast
```

### Semantic hints protocol

Clients send **action names**, not raw buttons. The overlay resolves each action to
its button binding via `bindings.toml`, then renders the correct controller icon.

```
bar:hints [{"action":"confirm","label":"select"},{"action":"back"},{"action":"queue"},{"action":"navigate"},{"action":"page_next"},{"action":"page_prev"}]
```

Each hint object:
- `action` (required): semantic action name from `bindings.toml` `[actions.*]`
- `label` (optional): display text override; falls back to the action's `label` in bindings.toml
- `hold` (optional): show hold indicator; falls back to action config

The overlay:
1. Looks up `action` in `bindings.toml` → gets `button`, `label`, `hold`, `hold_ms`
2. Uses client-provided `label` if present, otherwise the binding's label
3. Resolves `button` → controller icon (Xbox/PS/Switch auto-detected via `buttons.rs`)
4. Renders: `[icon] label` with optional hold-progress arc

This means romhoard never needs to know which physical button maps to what. The
binding config is the single source of truth. Kiosk-specific actions (queue, navigate,
filter, etc.) are defined in `bindings.toml` alongside the in-game actions.

**`bar:context`** sets a named context that loads a default set of hints from config.
**`bar:hints`** overrides those defaults with an explicit set. Use `bar:hints` for
dynamic pages where the available actions change based on state (e.g. different hints
when a game is owned vs not owned).

### Romhoard integration

Romhoard server gets a thin endpoint: `POST /api/overlay` that relays JSON to the
overlay socket. This way the webview JS doesn't need direct socket access:

```javascript
// In gamepad.js or page scripts:
fetch('/api/overlay', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cmd: 'bar:context detail' })
});

// Dynamic hints for a specific page state:
fetch('/api/overlay', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
        cmd: 'bar:hints',
        hints: [
            { action: 'confirm', label: 'launch' },
            { action: 'back' },
            { action: 'queue', label: 'download first' },
            { action: 'page_prev' },
            { action: 'page_next' }
        ]
    })
});
```

---

## Rendering: vello + wgpu + winit

### Why not tiny-skia

The overlay currently uses `tiny-skia` (CPU software rasterizer) with `fontdue` for
text. This was the right call when the overlay only rendered occasional toasts. With
persistent bars at 60fps, the calculus changes:

- **CPU→GPU copy tax**: tiny-skia renders to `Vec<u32>` in system memory, which
  gamescope then copies to a GPU texture for compositing. GPU rendering eliminates this.
- **Every-frame rendering**: Bars with clock/controllers/etc. update every frame.
  Software-rasterizing ~1920x120px 60 times/sec is fine at 1080p but scales linearly
  with resolution (4K = 4x work). GPU doesn't care.
- **Animation quality**: Bar slide-in/out, toast easing, glow effects — expensive on
  CPU, trivial as GPU shaders.
- **Text quality**: `fontdue` is functional but basic. `parley` (vello's text stack)
  gives proper shaping, font fallback, and subpixel positioning.

### Why vello, not a GUI framework

The overlay is a **render loop**, not an app:

```
loop { poll_events → update_state → draw_frame → present → sleep }
```

No text input, no scrolling lists, no interactive widgets, no layout reflow. It's a
game HUD. GUI frameworks (Tauri, Dioxus, iced, egui) add widget trees, layout engines,
event dispatch, accessibility — none of which apply here. They'd also fight the X11
atom registration and gamescope integration.

**vello** is a GPU-accelerated 2D rendering library (Linebender project). It's a
drawing API, not a framework — rounded rects, paths, text, images. No widget overhead.
Paired with **wgpu** for GPU abstraction and **winit** for windowing.

### Dependency stack

```
winit          → X11 window creation + event loop
wgpu           → Vulkan/OpenGL GPU abstraction (gamescope already requires Vulkan)
vello          → GPU-accelerated 2D path rendering
parley         → Text layout (shaping, font fallback, line breaking)
peniko         → Color/brush types shared across Linebender ecosystem
```

### Rendering architecture

```rust
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    vello_renderer: vello::Renderer,
    // Theme colors (same as current)
    fg: peniko::Color,
    bg: peniko::Color,
    accent: peniko::Color,
    card: peniko::Color,
    // Fonts
    display_font: parley::FontFamily,
    body_font: parley::FontFamily,
    light_font: parley::FontFamily,
    // Controller icons (pre-rasterized SVGs as vello::Scene fragments)
    button_icons: Option<ButtonIcons>,
}

impl Renderer {
    /// Build a vello Scene for one frame (replaces tiny-skia Pixmap).
    pub fn render_frame(&self, state: &FrameState, w: u32, h: u32) {
        let mut scene = vello::Scene::new();

        // Bars (always, unless InGame)
        if let Some(bars) = state.bars {
            self.draw_top_bar(&mut scene, bars, w);
            self.draw_bottom_bar(&mut scene, bars, w, h);
        }

        // Backdrop (menu open)
        if state.menu.map_or(false, |m| m.is_visible()) {
            self.draw_backdrop(&mut scene, w, h, state.menu.unwrap().opacity());
        }

        // Quick menu
        if let Some(menu) = state.menu {
            self.draw_menu(&mut scene, menu, state.menu_config, h);
        }

        // Achievement toast
        if let Some(popup) = state.popup {
            self.draw_toast(&mut scene, popup, w);
        }

        // Submit to GPU — no CPU pixel buffer, no copy
        let surface_texture = self.surface.get_current_texture().unwrap();
        self.vello_renderer.render_to_surface(
            &self.device, &self.queue, &scene,
            &surface_texture, &vello::RenderParams {
                base_color: peniko::Color::TRANSPARENT,
                width: w, height: h,
                antialiasing_method: vello::AaConfig::Msaa16,
            },
        ).unwrap();
        surface_texture.present();
    }
}
```

### Drawing example (bar panel)

```rust
fn draw_top_bar(&self, scene: &mut vello::Scene, bars: &Bars, screen_w: u32) {
    let w = screen_w as f64 - BAR_MARGIN_X as f64 * 2.0;
    let rect = kurbo::RoundedRect::new(
        BAR_MARGIN_X as f64, BAR_MARGIN_Y as f64,
        BAR_MARGIN_X as f64 + w, BAR_MARGIN_Y as f64 + BAR_INNER_H as f64,
        BAR_RADIUS as f64,
    );
    // Panel background
    scene.fill(
        vello::peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        self.card.with_alpha_factor(0.75),
        None, &rect,
    );
    // Profile cluster (left)
    self.draw_profile_cluster(scene, &bars.profile, BAR_MARGIN_X as f64 + 16.0, ...);
    // System cluster (right)
    self.draw_system_cluster(scene, &bars.system, screen_w as f64 - BAR_MARGIN_X as f64 - 16.0, ...);
}
```

### Migration path

The existing toast and menu rendering (tiny-skia) migrates incrementally:
1. **Phase 2a**: New bar code written directly with vello (no legacy)
2. **Phase 2b**: Port toast rendering from tiny-skia to vello
3. **Phase 2c**: Port menu rendering from tiny-skia to vello
4. **Phase 2d**: Remove tiny-skia dependency entirely

During migration, both renderers can coexist — vello for bars, tiny-skia for
toasts/menu composited via `vello::Scene::append` from a rasterized image.

---

## Architecture: New Module `bars.rs`

```rust
pub struct Bars {
    profile: ProfileState,
    system: SystemState,
    hints: HintState,
    context: BarContext,
    visible: bool,
    timers: BarTimers,
}

pub struct ProfileState {
    avatar: Option<vello::peniko::Image>,  // GPU-ready image
    username: String,
    balance: Option<String>,
    currency_symbol: String,
}

pub struct SystemState {
    clock: String,
    battery_pct: Option<u8>,
    battery_charging: bool,
    controller_count: u8,
    controller_active: [bool; 4],
    weather_icon: Option<vello::peniko::Image>,
    weather_temp: Option<String>,
    ra_user: Option<String>,
    net_connected: bool,
}

pub struct HintItem {
    action: String,               // semantic action name from bindings.toml
    label_override: Option<String>, // client-provided label, falls back to binding label
}

pub enum BarContext {
    Browse,
    Detail,
    Queue,
    Library,
    InGame,
    Settings,
    EsSystem,
    EsGamelist,
    Custom,
}

struct BarTimers {
    clock: Instant,         // 1s
    battery: Instant,       // 30s
    controllers: Instant,   // 500ms
    network: Instant,       // 30s
    file_watch: Instant,    // 5s (avatar, weather)
}
```

### Main loop (winit event loop)

```rust
// winit event loop replaces the manual loop + sleep:
event_loop.run(move |event, target| {
    match event {
        Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
            // Drain socket commands
            while let Ok(cmd) = rx.try_recv() {
                handle_command(&mut bars, &mut popup_queue, &mut game_menu, cmd);
            }

            // Tick state
            bars.tick_timers();
            popup_queue.tick();
            game_menu.tick();

            // Build frame
            let has_bars = bars.visible && bars.context != BarContext::InGame;
            let has_content = has_bars
                || popup_queue.current().is_some()
                || game_menu.is_visible();

            if has_content {
                renderer.render_frame(&FrameState {
                    bars: if has_bars { Some(&bars) } else { None },
                    popup: popup_queue.current(),
                    menu: if game_menu.is_visible() { Some(&game_menu) } else { None },
                    menu_config: &menu_config,
                    game_name: None,
                    bindings: Some(&bindings),
                }, screen_w, screen_h);
            }

            // Request next frame
            window.request_redraw();
        }
        _ => {}
    }
});
```

---

## Game Launching

Romhoard kiosk currently queues downloads. For launching, add a "launch" action:

```
POST /api/launch
{ "system": "snes", "rom": "/userdata/roms/snes/ff6.sfc" }
```

Romhoard server:
1. Sends `bar:context ingame` to overlay socket
2. Calls `batocera-run snes /userdata/roms/snes/ff6.sfc`
3. Waits for process exit
4. Sends `bar:context library` to overlay socket
5. Returns control to Chromium (which was still open underneath)

`batocera-run` handles emulator selection, per-game config, RetroArch setup — all the
complexity ES delegates to it. We call the same binary.

### In-game overlay

Already built: quick menu, achievement toasts, controller hints. The overlay stays
active during gameplay. Bars hide (InGame context), but the overlay window persists
for toasts and menu.

---

## Romhoard Kiosk Changes

### Remove from templates

- `.kiosk-topbar` — overlay owns this
- `.kiosk-hints` footer — overlay owns this
- Profile/currency display — overlay owns this

### Keep and enhance

- Full-viewport content (browse grid, game detail, queue, library)
- `gamepad.js` for focus management, navigation, and action dispatch
- Page-level context notifications to overlay (`bar:context <page>`)

### New pages needed

| Page | Route | Purpose |
|------|-------|---------|
| Library | `/kiosk/library` | Owned games, launch button, play stats |
| Settings | `/kiosk/settings` | Links to Batocera web manager, display/audio/network |
| ES fallback | `/kiosk/es` | Button to launch ES for people who prefer it |

### Transitions

For a "living room boot experience" that feels native:
- CSS transitions on page changes (fade, slide)
- Preload next page via HTMX `hx-boost` for instant navigation
- Loading states with themed spinners
- Gamepad.js manages focus transfer across page transitions

---

## Config (`bars.toml`)

```toml
[bars]
enabled = true
height = 60
margin_x = 16
margin_y = 10
corner_radius = 14
background_opacity = 0.75

[bars.profile]
show_avatar = true
show_balance = true
currency_symbol = "K"
avatar_path = "./generated/avatar.png"

[bars.system]
show_clock = true
clock_format = "%H:%M"
show_battery = true
show_weather = true
show_ra_user = true
show_controllers = true

[bars.hints]
default_context = "browse"

[session]
# What to autostart (replaces ES)
frontend = "romhoard"           # "romhoard" | "es" | "custom"
romhoard_url = "http://localhost:1337/kiosk"
browser = "chromium"
browser_flags = ["--kiosk", "--noerrdialogs", "--disable-translate"]

[gamescope]
enabled = true
output_width = 0                # 0 = auto-detect
output_height = 0
upscale_filter = "linear"       # "linear" | "fsr" | "nis" | "nearest"
adaptive_sync = false
```

---

## Implementation Order

### Phase 1: Rendering migration + gamescope
1. Add `wgpu`, `vello`, `parley`, `winit` to Cargo.toml
2. New `renderer_vello.rs` — wgpu surface setup, vello Scene building
3. Port `window.rs` from raw X11 to winit (keeps X11 backend, adds event loop)
4. Set `GAMESCOPE_EXTERNAL_OVERLAY` atom via winit's raw window handle
5. Build gamescope for Batocera (package + deps)
6. Write `superkonna-session` launch script
7. Test: gamescope wraps a test app, overlay registers as external overlay

### Phase 2: Bar rendering (new code, vello-native)
8. `bars.rs` module — state, contexts, timer ticks
9. `socket.rs` extensions — `bar:*` commands
10. `renderer_vello.rs` — `draw_top_bar`, `draw_bottom_bar`, clusters, hints
11. `main.rs` — winit event loop, always-visible bars, timer polling
12. `config.rs` — bars.toml parsing
13. Port toast rendering from tiny-skia to vello
14. Port menu rendering from tiny-skia to vello
15. Remove tiny-skia + fontdue dependencies

### Phase 3: Romhoard as primary frontend
16. Strip topbar/footer from kiosk templates
17. Add `/api/overlay` relay endpoint
18. Add `/api/launch` endpoint (calls `batocera-run`)
19. Add `/kiosk/library` page (owned games, launch action)
20. Add `/kiosk/settings` page (links to Batocera web manager)
21. Page context notifications on navigation

### Phase 4: Batocera integration
22. Autostart script (replaces ES in boot sequence)
23. Batocera overlay (filesystem) packaging for custom image
24. RetroAchievements integration verification (already wired)
25. batocera-run lifecycle testing (blocking behavior, exit codes)
26. Chromium kiosk setup (flags, GPU acceleration, touch/gamepad)

### Phase 5: Polish
27. CSS page transitions (fade/slide between kiosk pages)
28. Gamepad.js focus management across page transitions
29. Boot splash screen (overlay renders themed splash while romhoard starts)
30. ES as fallback: launchable from settings page
31. Error recovery: if Chromium crashes, restart loop
32. Bar animations: slide-in/out on context transitions

---

## Existing Overlay Features (already built, carry forward)

These modules are already implemented and working. They carry forward into the new
rendering stack with minimal changes (just swap tiny-skia draw calls to vello):

| Module | What it does | Status |
|--------|-------------|--------|
| `watcher.rs` | Tails RetroArch log for RetroAchievements events | Done |
| `popup.rs` | Toast animation state machine (SlideIn→Hold→FadeOut→Done) | Done |
| `menu.rs` | Quick menu state machine (Closed→Opening→Open→Confirming→Closing) | Done |
| `buttons.rs` | Controller detection (Xbox/PS/Switch/SteamDeck) + SVG icon rasterization | Done |
| `bindings.rs` | Unified input bindings from `bindings.toml` | Done |
| `socket.rs` | Unix socket at `/tmp/superkonna-overlay.sock` | Done (extend for bar:*) |
| `audio.rs` | Sound playback for menu/achievement events | Done |
| `theme.rs` | ES theme XML color/font loading with color-scheme switching | Done |
| `config.rs` | Menu TOML config with fallback chain | Done (extend for bars) |
| `retroarch.rs` | UDP command client for RetroArch network commands | Done |

RetroAchievements are fully wired: log watcher detects unlock events, popup queue
manages toast display, renderer draws themed notification cards. This works independent
of whether ES or romhoard is the frontend — it watches RetroArch's log directly.

---

## Open Questions

1. **Gamescope viewport centering**: When inner AR differs from output, gamescope may
   top-align. Need to test `-S fit` or contribute a centering patch.

2. **Webview in gamescope**: The GTK3+WebKit2 kiosk webview (`kiosk-webview.py`) works on
   Batocera v42+ natively. Need to verify it works inside gamescope's nested compositor.

3. **batocera-run integration**: Does it block until the emulator exits? Need to verify
   the process lifecycle so romhoard can detect game-end reliably.

4. **Boot time**: Romhoard + Chromium startup may be slower than ES. A themed splash
   screen (rendered by the overlay itself) during boot would mask this.

5. **Multi-user**: If romhoard supports multiple profiles, the overlay needs to update
   profile cluster on user switch. Socket protocol already supports this.

6. **ES theme maintenance**: With ES as fallback only, do we keep maintaining the theme?
   Minimal maintenance — it works, just no new features.

7. **vello maturity**: vello is pre-1.0. Need to pin a known-good version and test on
   Batocera's Vulkan stack. Fallback: tiny-skia stays as a compile-time feature flag.

8. **winit + gamescope**: Does winit's X11 backend properly expose the window for
   gamescope's external overlay detection? May need raw-window-handle to set the atom.
