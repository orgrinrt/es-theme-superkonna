# Batocera Integration Guide

How to replace EmulationStation with romhoard + superkonna-overlay on Batocera Linux.

---

## Key Discovery: There Is No `batocera-run`

The game launch mechanism is **`emulatorlauncher.py`**, a Python script. Any process
can call it — no special context, no ES dependency.

```bash
python /usr/lib/python3.12/site-packages/configgen/emulatorlauncher.py \
  -system snes \
  -rom "/userdata/roms/snes/game.smc" \
  -emulator libretro \
  -core snes9x
```

### CLI arguments

**Required:**
- `-system` — system identifier (`snes`, `psx`, `n64`, etc.)
- `-rom` — full path to ROM file

**Optional:**
- `-emulator` — force emulator (e.g., `libretro`, `dolphin-emu`); defaults from `batocera.conf`
- `-core` — force core (e.g., `snes9x`, `bsnes`); defaults from `batocera.conf`
- `-state_slot`, `-state_filename`, `-autosave` — save state control
- `-lightgun`, `-wheel`, `-trackball`, `-spinner` — peripheral flags

**Controller config (per player, P1-P8):**
- `-p{N}index`, `-p{N}guid`, `-p{N}name`, `-p{N}devicepath`
- `-p{N}nbbuttons`, `-p{N}nbhats`, `-p{N}nbaxes`

Controller args are optional — omit for no explicit controller mapping.

### Behavior

- **Blocks until emulator exits** (`proc.communicate()`)
- Returns emulator's exit code (0 = normal, negative = signal)
- Sleeps 1s before exit "so that the gpu memory is restituated and available for es"
- Handles all config generation, controller mapping, video mode changes
- Runs `gameStart`/`gameStop` event scripts automatically
- Sets env vars: `SDL_RENDER_VSYNC`, `MANGOHUD_DLSYM`, `MANGOHUD_CONFIGFILE`

### Internal flow

1. Extract squashfs ROMs if needed
2. Load controller config → `Controller` objects
3. Resolve emulator/core via priority chain:
   - `configgen-defaults.yml` → `configgen-defaults-arch.yml` → `batocera.conf` → CLI args
4. Select generator class (e.g., `libretroGenerator`, `dolphinGenerator`)
5. Generator produces `Command` object (argv + env dict)
6. Adjust video mode if needed
7. Set up MangoHUD/bezels
8. Run pre-launch hooks
9. `subprocess.Popen(command.array)` → `proc.communicate()` (blocks)
10. Restore video mode, run post-launch hooks

### Calling from romhoard

```rust
// In romhoard's launch handler:
use std::process::Command;

let status = Command::new("python")
    .args([
        "/usr/lib/python3.12/site-packages/configgen/emulatorlauncher.py",
        "-system", &system,
        "-rom", &rom_path,
    ])
    .status()  // blocks until exit
    .expect("failed to launch game");

// status.code() gives exit code
```

The Python path varies by Batocera version (3.9, 3.11, 3.12). Detect at startup:
```bash
python_path=$(find /usr/lib/python3.*/site-packages/configgen/emulatorlauncher.py 2>/dev/null | head -1)
```

Source: [emulatorlauncher.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/emulatorlauncher.py)

---

## Boot Sequence

Batocera uses sysvinit with numbered init.d scripts. Key stages:

| Script | Purpose |
|--------|---------|
| `S00bootcustom` | Runs `/boot/boot-custom.sh` (earliest user hook) |
| `S11share` | Mounts `/userdata` partition |
| `S12populateshare` | Populates `/userdata` defaults (slow on first boot) |
| `S31emulationstation` | Starts ES (installed by `batocera-emulationstation` package) |
| `S99userservices` | Runs user services + legacy `custom.sh` |

### How ES starts

`S31emulationstation` runs the `emulationstation-standalone` wrapper script which:

1. Sets `HOME=/userdata/system`
2. Detects Wayland vs X11
3. Waits for compositor readiness
4. Configures DPI, keyboard layout, touchscreen
5. **Launches ES in a restart loop:**
   ```bash
   dbus-run-session -- emulationstation ${GAMELAUNCHOPT} --exit-on-reboot-required
   ```
6. After ES exits, checks control files:
   - `/tmp/es-restart` → restart ES
   - `/tmp/es-sysrestart` → `sudo reboot`
   - `/tmp/es-shutdown` → `sudo poweroff`

### How to replace ES

**Option A: `/boot/postshare.sh`** (recommended)

Runs after `/userdata` mounts, before ES starts. Kill or prevent ES, start our session:

```bash
#!/bin/bash
# /boot/postshare.sh — runs before S31emulationstation

# Prevent ES from starting by creating a flag file
touch /tmp/superkonna-active

# Start our session instead (gamescope + overlay + romhoard)
/userdata/superkonna/superkonna-session &
```

Then modify (via overlay) `S31emulationstation` to check:
```bash
[ -f /tmp/superkonna-active ] && exit 0
```

**Option B: Replace `S31emulationstation` entirely**

Use Batocera's filesystem overlay to replace the init script:

```bash
# On a running Batocera system:
cp /userdata/superkonna/S31superkonna /etc/init.d/S31emulationstation
batocera-save-overlay
```

**Option C: User service**

Add to `/userdata/system/services/superkonna`:
```bash
#!/bin/bash
start() { /userdata/superkonna/superkonna-session & }
stop() { killall superkonna-session superkonna-overlay romhoard chromium; }
```

Enable via Batocera web manager or `batocera-services enable superkonna`.

### Keeping ES as fallback

ES stays installed. Our settings page can launch it:
```bash
# Kill our session, trigger ES restart
rm -f /tmp/superkonna-active
touch /tmp/es-restart
killall chromium superkonna-overlay
# The ES restart loop in emulationstation-standalone picks up /tmp/es-restart
```

---

## Filesystem Layout

Batocera's rootfs is **read-only squashfs**. Writable paths:

| Path | Type | Survives reboot |
|------|------|----------------|
| `/userdata/` | Separate partition, rw | Yes |
| `/boot/` | FAT32, rw | Yes |
| `/` (rootfs) | squashfs + overlay | Only with `batocera-save-overlay` |

### Where to put our binaries

**Recommended: `/userdata/superkonna/`**

```
/userdata/superkonna/
  superkonna-overlay        # Rust binary (overlay renderer)
  superkonna-session        # Bash script (session launcher)
  romhoard                  # Rust binary (web server)
  chromium/                 # Browser binary + libs (if needed)
  config/
    bars.toml
    menu.toml
    bindings.toml
  theme -> /userdata/themes/es-theme-superkonna/  # symlink
```

Add to PATH via the session script rather than modifying system PATH.

### Filesystem overlay (for init script changes)

```bash
# Make rootfs writable (overlay does this at boot, but for manual changes):
mount -o remount,rw /

# Install custom init script
cp S31superkonna /etc/init.d/S31emulationstation

# Save overlay (max 50MB default, expandable)
batocera-save-overlay

# To remove overlay and restore stock:
rm /boot/boot/overlay
reboot
```

---

## Webview: Already Solved

**Batocera v42+ ships GTK3 + WebKit2.** Romhoard already has a kiosk webview wrapper:

```
romhoard/scripts/batocera/kiosk-webview.py
```

63-line Python script using `gi.repository` → `Gtk.Window` + `WebKit2.WebView`:
- Fullscreen, hidden cursor, escape-to-exit
- Hardware acceleration policy configurable
- Context menu disabled for kiosk feel
- Loads `http://127.0.0.1:9090/kiosk` by default (URL from argv)

No Chromium, no CEF, no browser bundling. GTK3 and WebKit2 are already on the image.

### For the gamescope session

The session script launches this webview instead of Chromium:

```bash
python3 /userdata/superkonna/kiosk-webview.py "http://localhost:1337/kiosk"
```

### Potential enhancement

The current wrapper has `HardwareAccelerationPolicy.NEVER`. Inside gamescope with
Vulkan available, we could enable GPU acceleration:

```python
settings.set_hardware_acceleration_policy(
    WebKit2.HardwareAccelerationPolicy.ALWAYS
)
```

Test whether this conflicts with gamescope's compositing or improves scrolling
performance on the kiosk pages.

---

## Gamescope on Batocera

### Build recipe (from PR #13690, updated)

```makefile
# package/batocera/utils/gamescope/gamescope.mk
GAMESCOPE_VERSION = 3.16.20
GAMESCOPE_SITE = https://github.com/ValveSoftware/gamescope
GAMESCOPE_SITE_METHOD = git
GAMESCOPE_GIT_SUBMODULES = YES
GAMESCOPE_DEPENDENCIES = sdl2 libdrm wayland wayland-protocols glm hwdata \
    pipewire xlib_libXres xlib_libXmu stb seatd xwayland libdecor \
    vulkan-headers vulkan-loader
GAMESCOPE_CONF_OPTS += -Denable_openvr_support=FALSE

define GAMESCOPE_INSTALL_TARGET_CMDS
    mkdir -p $(TARGET_DIR)/usr/bin
    $(INSTALL) -D $(@D)/build/src/gamescope $(TARGET_DIR)/usr/bin/gamescope
endef

$(eval $(meson-package))
```

### libseat fix

Ensure Batocera's seatd is built with the right backend. Since Batocera uses sysvinit
(no systemd), the logind backend is unavailable. Two options:

- `BR2_PACKAGE_SEATD_DAEMON=y` → builds seatd daemon + `-Dlibseat-seatd=enabled`
- `BR2_PACKAGE_SEATD_BUILTIN=y` → builds builtin backend (no daemon needed)

Batocera's `batocera-system/Config.in` already selects `BR2_PACKAGE_SEATD_DAEMON`.
If builds fail, verify this is set.

### Vulkan availability

Batocera x86_64 includes Vulkan drivers:
- **AMD**: Mesa RADV
- **Intel**: Mesa ANV
- **NVIDIA**: Proprietary driver

Both gamescope and wgpu work with these. No additional Vulkan packages needed.

### Invocation for our use case

```bash
gamescope \
  -f \                    # fullscreen
  -W $SCREEN_W \          # output width (display native)
  -H $SCREEN_H \          # output height (display native)
  -w $SCREEN_W \          # inner width (same as output)
  -h $VIEWPORT_H \        # inner height (output - 2*BAR_H)
  -S fit \                # letterbox (centers viewport, black bars top/bottom)
  -r 60 \                 # framerate cap
  -- superkonna-session
```

`-S fit` centers the inner viewport and adds black bars. Our overlay fills those bars.

### GAMESCOPE_EXTERNAL_OVERLAY atom

Set on our X11 window to register as an overlay:

```rust
// Using x11rb crate (or via winit's raw-window-handle):
let atom = conn.intern_atom(false, b"GAMESCOPE_EXTERNAL_OVERLAY")?.reply()?.atom;
conn.change_property32(
    PropMode::REPLACE,
    window_id,
    atom,
    AtomEnum::CARDINAL,
    &[1],
)?;
conn.flush()?;
```

The overlay window renders at full output resolution (1920x1080), above the game
(z=2), below cursor (z=4). Not subject to game scaling (`NoScale | NoFilter`).

**VRR caveat**: When VRR is active, external overlays are prevented from repainting
the base plane to avoid frame pacing issues. Not a concern for our 60fps bars.

---

## Session Script

```bash
#!/bin/bash
# /userdata/superkonna/superkonna-session
# Launched by gamescope as the inner session.

set -euo pipefail

SUPERKONNA_DIR="/userdata/superkonna"
THEME_ROOT="/userdata/themes/es-theme-superkonna"
ROMHOARD_URL="http://localhost:1337/kiosk"
OVERLAY_SOCK="/tmp/superkonna-overlay.sock"

# Export for overlay to find theme
export SUPERKONNA_THEME_ROOT="$THEME_ROOT"

# Start overlay (registers as GAMESCOPE_EXTERNAL_OVERLAY)
"$SUPERKONNA_DIR/superkonna-overlay" &
OVERLAY_PID=$!

# Wait for overlay socket to appear
for i in $(seq 1 30); do
    [ -S "$OVERLAY_SOCK" ] && break
    sleep 0.1
done

# Start romhoard server if not already running
if ! pgrep -f romhoard >/dev/null; then
    "$SUPERKONNA_DIR/romhoard" &
    ROMHOARD_PID=$!
    # Wait for server to be ready
    for i in $(seq 1 50); do
        curl -sf "$ROMHOARD_URL" >/dev/null 2>&1 && break
        sleep 0.1
    done
fi

# Launch kiosk webview (GTK3 + WebKit2, pre-installed on Batocera v42+)
python3 "$SUPERKONNA_DIR/kiosk-webview.py" "$ROMHOARD_URL"

# If webview exits (Escape key or crash), clean up
kill $OVERLAY_PID 2>/dev/null || true
kill $ROMHOARD_PID 2>/dev/null || true
```

---

## ES Event Scripts (interop)

ES fires event scripts at:
```
/userdata/system/configs/emulationstation/scripts/<event>/<script>.sh
```

Events: `game-start`, `game-end`, `system-selected`, etc. Scripts must be `chmod +x`.

If ES is running as fallback, these can notify our overlay:

```bash
#!/bin/bash
# /userdata/system/configs/emulationstation/scripts/game-start/notify-overlay.sh
echo "bar:context ingame" | socat - UNIX:/tmp/superkonna-overlay.sock
```

```bash
#!/bin/bash
# /userdata/system/configs/emulationstation/scripts/game-end/notify-overlay.sh
echo "bar:context es-gamelist" | socat - UNIX:/tmp/superkonna-overlay.sock
```

---

## RetroAchievements

**Already fully handled by the overlay.** The existing pipeline:

1. `watcher.rs` tails `/tmp/retroarch.log` for RA unlock events
2. `popup.rs` manages toast animation (SlideIn → Hold → FadeOut → Done)
3. `renderer.rs` draws themed notification cards
4. `audio.rs` plays achievement sound

This works regardless of whether ES or romhoard launched the game — it watches
RetroArch's log directly, not ES events.

For the romhoard frontend, we may want to also:
- Show RA user profile in the top bar (socket: `bar:ra_user <name>`)
- Show recent achievements on the library page
- Link to RetroAchievements profile from settings

These are romhoard features, not overlay changes.

---

## Cross-Compilation

Batocera x86_64 uses **glibc** (not musl). Target triple: `x86_64-unknown-linux-gnu`.

### Using `cross` (from macOS or any host)

```bash
# Install cross
cargo install cross

# Build overlay
cd projects/overlay
cross build --release --target x86_64-unknown-linux-gnu

# Build romhoard
cd ~/Dev/romhoard
cross build --release --target x86_64-unknown-linux-gnu
```

`cross` uses Docker containers matching the target libc. Ensure the container has
Vulkan headers and wgpu build deps.

### Using Batocera's Buildroot toolchain

For tighter integration, add Rust packages to the Buildroot config. Buildroot has
`cargo-package` infrastructure for Rust crates.

### Deploying binaries

```bash
# SCP to Batocera box (default root password: empty or "linux")
scp target/x86_64-unknown-linux-gnu/release/superkonna-overlay root@batocera:/userdata/superkonna/
scp target/x86_64-unknown-linux-gnu/release/romhoard root@batocera:/userdata/superkonna/
```

---

## batocera-es-swissknife (management script)

Useful for controlling ES from our session:

```bash
batocera-es-swissknife --restart    # Kill ES (auto-restarts via loop)
batocera-es-swissknife --espid      # Check if ES is running
batocera-es-swissknife --emukill    # Kill running emulator
```

Exit codes: 20 (hotkeygen), 22 (force-kill), 21 (no emulator), 10/11 (system cmds).

---

## Summary: What We Need to Build/Package

| Component | Status | Size | Location |
|-----------|--------|------|----------|
| superkonna-overlay | Exists, needs vello migration + bars | ~5MB | `/userdata/superkonna/` |
| romhoard | Exists, needs launch endpoint | ~10MB | `/userdata/superkonna/` |
| gamescope | Build from source for Batocera | ~5MB | `/usr/bin/` (overlay) or `/userdata/superkonna/` |
| kiosk-webview.py | Exists (`romhoard/scripts/batocera/`) | ~2KB | `/userdata/superkonna/` |
| superkonna-session | New bash script | ~2KB | `/userdata/superkonna/` |
| S31 init override | New init script | ~1KB | `/etc/init.d/` (overlay) |
| bars.toml | New config | ~1KB | `/userdata/superkonna/config/` |
| ES event scripts | New, for fallback interop | ~200B each | `/userdata/system/configs/emulationstation/scripts/` |
| GTK3 + WebKit2 | Pre-installed on Batocera v42+ | 0 (system) | System libs |
