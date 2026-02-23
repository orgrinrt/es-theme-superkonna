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
    avatar: Option<RasterizedImage>,
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
    weather_icon: Option<RasterizedImage>,
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

### Renderer additions

```rust
// In renderer.rs — new methods:
fn draw_top_bar(&self, pixmap: &mut Pixmap, bars: &Bars, screen_w: u32)
fn draw_bottom_bar(&self, pixmap: &mut Pixmap, bars: &Bars, screen_w: u32, screen_h: u32)
fn draw_bar_panel(&self, pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32)
fn draw_profile_cluster(&self, pixmap: &mut Pixmap, profile: &ProfileState, x: f32, cy: f32)
fn draw_system_cluster(&self, pixmap: &mut Pixmap, system: &SystemState, right_edge: f32, cy: f32)
fn draw_hints(&self, pixmap: &mut Pixmap, hints: &[HintItem], x: f32, cy: f32, max_w: f32)
```

### Main loop changes

```rust
// Bars render ALWAYS when visible (not just popup/menu):
let has_bars = bars.visible && bars.context != BarContext::InGame;

// Tick bar timers:
bars.tick_timers();

// Frame always needs drawing if bars are up:
if has_bars || has_popup || has_menu {
    let frame_state = FrameState {
        popup: popup_queue.current(),
        menu: if has_menu { Some(&game_menu) } else { None },
        menu_config: &menu_config,
        game_name: None,
        bindings: Some(&bindings),
        bars: if has_bars { Some(&bars) } else { None },
    };
    let pixels = rend.render_frame(&frame_state, screen_w as u32, screen_h as u32);
    win.show();
    win.update_pixels(&pixels, screen_w, screen_h);
} else if !has_bars {
    // In-game: only show for popup/menu
    if !has_popup && !has_menu {
        win.hide();
    }
}
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

### Phase 1: Gamescope wrapper + session script
1. Build gamescope for Batocera (package + deps)
2. Write `superkonna-session` launch script
3. Test: gamescope wraps Chromium, overlay registers as external overlay
4. Verify letterboxing works (inner 960px, bars in remaining space)

### Phase 2: Bar rendering
5. `bars.rs` module — state, contexts, timer ticks
6. `socket.rs` extensions — `bar:*` commands
7. `renderer.rs` — `draw_top_bar`, `draw_bottom_bar`, profile/system/hint clusters
8. `main.rs` — always-visible bars, timer polling
9. `config.rs` — bars.toml parsing

### Phase 3: Romhoard as primary frontend
10. Strip topbar/footer from kiosk templates
11. Add `/api/overlay` relay endpoint
12. Add `/api/launch` endpoint (calls `batocera-run`)
13. Add `/kiosk/library` page (owned games, launch action)
14. Add `/kiosk/settings` page (links to Batocera web manager)
15. Page context notifications on navigation

### Phase 4: Polish
16. CSS page transitions (fade/slide between kiosk pages)
17. Gamepad.js focus management across page transitions
18. Boot experience: splash screen while romhoard starts
19. ES as fallback: launchable from settings page
20. Error recovery: if Chromium crashes, restart loop

---

## Open Questions

1. **Gamescope viewport centering**: When inner AR differs from output, gamescope may
   top-align. Need to test `-S fit` or contribute a centering patch.

2. **Chromium on Batocera**: Does the Batocera image include Chromium? If not, need to
   bundle it. Alternatively, use the existing ES webview engine (but that limits us).

3. **batocera-run integration**: Does it block until the emulator exits? Need to verify
   the process lifecycle so romhoard can detect game-end reliably.

4. **Boot time**: Romhoard + Chromium startup may be slower than ES. A themed splash
   screen (rendered by the overlay itself) during boot would mask this.

5. **Multi-user**: If romhoard supports multiple profiles, the overlay needs to update
   profile cluster on user switch. Socket protocol already supports this.

6. **ES theme maintenance**: With ES as fallback only, do we keep maintaining the theme?
   Minimal maintenance — it works, just no new features.
