# Batocera configgen Pipeline — Research for Rust Rewrite

## Scope Note

This doc was researched against Batocera's configgen. Loisto builds its own OS — the
Rust configgen replaces Batocera's Python entirely. Batocera-specific paths (`/userdata/`),
tools (`batocera-resolution`, `batocera-evmapy`, `emulatorlauncher.py`), and ES-specific
formats (`es_input.cfg`) documented here are reference for understanding the problem space,
not our implementation targets. Our filesystem root is `/data/` instead of `/userdata/`.

## TL;DR

Batocera's `emulatorlauncher.py` is the orchestrator between ES and emulators. It receives
system+ROM+controller info as CLI args, loads a multi-level config cascade (YAML defaults →
`batocera.conf` globals → per-system → per-folder → per-game), dispatches to one of ~100+
generator classes that write emulator-specific config files, then launches the process. The
Generator trait contract is simple: receive context, write configs, return a command line.
A Rust rewrite can make the config cascade data-driven and replace Python's dynamic dispatch
with trait objects or an enum.

---

## Entry Point: emulatorlauncher.py

### How ES Invokes It

EmulationStation calls configgen as a Python module. The entry point is `launch()` in
`emulatorlauncher.py`.

Source: `batocera-linux/batocera.linux` repo under
`package/batocera/core/batocera-configgen/configgen/configgen/`

### Full CLI Arguments

```
Required:
  -system          str     System name ("snes", "psx", "n64", etc.)
  -rom             Path    Absolute path to ROM file

Optional:
  -emulator        str     Force specific emulator
  -core            str     Force specific core
  -netplaymode     str     "host" / "client" / "spectator"
  -netplaypass     str     Netplay password
  -netplayip       str     Remote IP
  -netplayport     str     Remote port
  -netplaysession  str     Session name
  -state_slot      str     Save state slot number
  -state_filename  str     State filename
  -autosave        str     Autosave flag
  -systemname      str     System display name
  -gameinfoxml     str     Path to game info XML (default: /dev/null)

Flags:
  -lightgun                Enable lightgun config
  -wheel                   Enable wheel config
  -trackball               Enable trackball
  -spinner                 Enable spinner

Per-player (1-8):
  -p{N}index       int     Controller device index
  -p{N}guid        str     SDL2 GUID
  -p{N}name        str     Controller name
  -p{N}devicepath  str     /dev/input/eventX
  -p{N}nbbuttons   int     Number of buttons
  -p{N}nbhats      int     Number of hats
  -p{N}nbaxes      int     Number of axes
```

**Real invocation from ES:**
```bash
python -m configgen.emulatorlauncher \
  -system snes -rom "/userdata/roms/snes/game.sfc" \
  -p1index 0 -p1guid "030000004c..." -p1name "PS4 Controller" \
  -p1devicepath /dev/input/event3 -p1nbbuttons 16 -p1nbhats 1 -p1nbaxes 6
```

### Key Environment Variables

| Variable | Purpose |
|----------|---------|
| `SDL_RENDER_VSYNC` | VSync toggle |
| `XDG_CONFIG_HOME` | Config base dir (`/userdata/system/configs`) |
| `SDL_GAMECONTROLLERCONFIG` | Generated SDL controller DB string |
| `MANGOHUD_DLSYM`, `MANGOHUD_CONFIGFILE` | HUD overlay |

---

## Package Structure

```
configgen/configgen/
├── emulatorlauncher.py          Entry point + orchestration
├── Emulator.py                  Emulator dataclass (config cascade)
├── Command.py                   Command dataclass (array + env)
├── controller.py                Controller dataclass + SDL mapping
├── controllersConfig.py         Device enumeration (pyudev)
├── config.py                    Config / SystemConfig (dict-like)
├── input.py                     Input dataclass
├── gun.py                       Gun detection + precalibration
├── exceptions.py                Exception hierarchy with exit codes
├── batoceraPaths.py             All filesystem path constants
├── types.py                     TypedDicts (Resolution, BezelInfo, etc.)
│
├── settings/
│   └── unixSettings.py          batocera.conf parser
│
├── utils/
│   ├── videoMode.py             Resolution switching
│   ├── bezels.py                Bezel image manipulation
│   ├── wheelsUtils.py           Steering wheel remapping
│   ├── evmapy.py                Pad-to-keyboard daemon
│   ├── hotkeygen.py             Hotkey daemon context manager
│   ├── squashfs.py              SquashFS ROM mount/unmount
│   ├── metadata.py              Game metadata from gamesdb.xml
│   ├── vulkan.py                Vulkan GPU detection
│   ├── gun_borders.py           Gun border drawing
│   ├── wine.py                  Wine prefix management
│   └── ...
│
└── generators/
    ├── Generator.py             Abstract base class
    ├── importer.py              Generator dispatch map
    │
    ├── libretro/                RetroArch (most complex)
    │   ├── libretroGenerator.py
    │   ├── libretroConfig.py       ~800+ lines of settings
    │   ├── libretroControllers.py  Controller mapping
    │   ├── libretroOptions.py      Per-core option blocks
    │   ├── libretroMAMEConfig.py
    │   ├── libretroRetroarchCustom.py
    │   └── libretroPaths.py
    │
    ├── dolphin/                 Wii/GameCube
    ├── pcsx2/                   PS2
    ├── ppsspp/                  PSP
    ├── cemu/                    Wii U
    ├── rpcs3/                   PS3
    ├── duckstation/             PS1
    ├── flycast/                 Dreamcast
    ├── mame/                    Standalone MAME
    ├── mupen/                   N64
    ├── melonds/                 DS
    ├── xemu/                    Xbox
    ├── ryujinx/                 Switch
    ├── wine/                    Windows games
    ├── steam/                   Steam
    └── ... (~100+ total)
```

---

## Generator Base Class

```python
class Generator(metaclass=ABCMeta):
    @abstractmethod
    def generate(
        self,
        system: Emulator,
        rom: Path,
        playersControllers: Controllers,
        metadata: Mapping[str, str],
        guns: Guns,
        wheels: DeviceInfoMapping,
        gameResolution: Resolution,
    ) -> Command:
        ...

    @abstractmethod
    def getHotkeysContext(self) -> HotkeysContext:
        ...

    # Optional overrides with defaults:
    def getResolutionMode(self, config) -> str: ...       # default: config['videomode']
    def getMouseMode(self, config, rom) -> bool: ...      # default: False
    def executionDirectory(self, config, rom) -> Path: ... # default: None
    def supportsInternalBezels(self) -> bool: ...          # default: False
    def hasInternalMangoHUDCall(self) -> bool: ...         # default: False
    def getInGameRatio(self, config, res, rom) -> float: ...  # default: 4/3
```

**Contract:**
- `generate()` receives all context, writes config files, returns `Command`
- `Command` = `{ array: [str], env: {str: str} }` — the executable + args + environment
- `getHotkeysContext()` returns hotkey definitions for the `hotkeygen` daemon

**HotkeysContext format:**
```python
{
    "name": "retroarch",
    "keys": {
        "exit": ["KEY_LEFTSHIFT", "KEY_ESC"],
        "menu": ["KEY_LEFTSHIFT", "KEY_F1"],
        "pause": ["KEY_LEFTSHIFT", "KEY_P"],
        "save_state": ["KEY_LEFTSHIFT", "KEY_F3"],
    }
}
```

---

## Configuration Cascade

This is the most important subsystem. Settings are resolved through a multi-level
merge, lowest to highest priority:

```
1. configgen-defaults.yml          (base defaults per system)
2. configgen-defaults-{arch}.yml   (architecture-specific overrides)
3. batocera.conf  global.*         (user global preferences)
4. batocera.conf  {system}.*       (user per-system)
5. batocera.conf  {system}.folder["path"].*  (user per-folder)
6. batocera.conf  {system}["game"].*         (user per-game)
7. CLI overrides  -emulator, -core           (highest priority)
```

### configgen-defaults.yml (excerpt)

```yaml
snes:
  emulator: libretro
  core: snes9x
  ratio: auto
  smooth: false
  rewind: false
  autosave: false

psx:
  emulator: libretro
  core: mednafen_psx_hw
  ratio: auto

n64:
  emulator: libretro
  core: mupen64plus-next
```

Architecture-specific overrides (e.g., ARM devices get different default cores):
```yaml
# configgen-defaults-aarch64.yml
n64:
  core: parallel_n64
psx:
  core: pcsx_rearmed
```

### batocera.conf format

INI-like, no section headers. Dot-namespaced keys:

```ini
## Global
global.videomode=default
global.ratio=auto
global.shaderset=sharp-bilinear-simple
global.rewind=false
global.bezel=consoles
global.retroachievements=true
global.retroachievements.username=myuser
global.retroachievements.password=mypass

## Per-system
snes.core=bsnes
snes.ratio=4/3
snes.smooth=false

## Per-game
snes["Super Mario World.sfc"].core=snes9x
snes["Super Mario World.sfc"].ratio=auto

## Per-folder
snes.folder["/userdata/roms/snes/hacks"].core=snes9x_next

## Display
display.rotate=0
display.vsync=true

## Controllers
controllers.guns.borderssize=medium
```

### The Merge (Emulator.__post_init__)

The `Emulator` dataclass constructor performs:

1. Load `configgen-defaults.yml`
2. Load `configgen-defaults-{arch}.yml`
3. Recursive merge of defaults
4. Load `batocera.conf` via `UnixSettings`
5. Extract global, system, folder, and game settings
6. Merge in order: defaults < global < system < folder < game
7. Apply CLI overrides
8. Load ES settings (showFPS, UIMode)
9. Load rendering/shader config from `rendering-defaults.yml`

Result: `system.config` — a flat `SystemConfig` dict-like with all resolved settings.

### SystemConfig

```python
class SystemConfig(dict):
    @property
    def emulator(self) -> str: ...
    @property
    def core(self) -> str: ...
    def get_bool(self, key, default=False) -> bool: ...
    def get_str(self, key, default='') -> str: ...
    # Also supports: system.config['ratio'], system.config.get('smooth', 'false')
```

---

## Full Orchestration Flow

Complete trace from "user selects game" to "emulator exits":

### Pre-launch

1. **Parse CLI args** — system, ROM, controllers, flags
2. **SquashFS mount** — if ROM is `.squashfs`, mount to `/var/run/squashfs/`
3. **Load controllers** — `Controller.load_for_players()` reads `es_input.cfg`, builds
   `Controller` objects for each player
4. **Start controller monitor** — background thread watches for connect/disconnect via pyudev
5. **Build Emulator** — construct `Emulator(args, rom)`, triggers the config cascade
6. **Load metadata** — game info from `gamesdb.xml` (cheevosId, name, etc.)
7. **Gun detection** — scan for lightguns via pyudev/evdev, copy precalibration NVRAM
8. **Wheel configuration** — detect physical wheels, remap inputs, spawn evsieve if needed
9. **Get generator** — lookup in `_GENERATOR_MAP` by emulator name
10. **Resolution switch** — via `batocera-resolution` CLI
11. **Create save dirs** — `/userdata/saves/{system}/`
12. **Mouse mode** — show/hide cursor
13. **SDL VSync** — set `SDL_RENDER_VSYNC` env var
14. **Pre-launch scripts** — run `/usr/share/batocera/configgen/scripts/` with `gameStart`
15. **Evmapy** — start pad-to-keyboard daemon if needed (context manager)
16. **Hotkeygen** — set hotkey context for this generator (context manager)
17. **Working directory** — generator can specify `executionDirectory()`

### Launch

18. **`generator.generate()`** — THE CORE: writes all config files, returns `Command`
19. **HUD/Bezel** — select bezel image, resize, generate MangoHUD config
20. **Gun overlays** — help image and borders if applicable
21. **`subprocess.Popen(command.array, env=command.env)`** — launch emulator
22. **`proc.communicate()`** — block until emulator exits

### Post-launch

23. **Post-launch scripts** — run with `gameStop` event
24. **Resolution restore** — switch back to original mode
25. **Mouse restore** — restore cursor state
26. **Evmapy stop** — context manager cleanup
27. **Hotkeygen reset** — context manager cleanup
28. **SquashFS unmount** — context manager cleanup
29. **1-second sleep** — GPU memory recovery
30. **Exit code normalization** — negative (signal) → 0

---

## RetroArch Generator (Deep Dive)

The most complex generator. Lives in `generators/libretro/`.

### Core Selection

Determined by defaults + overrides:
```yaml
# configgen-defaults.yml
snes:
  emulator: libretro
  core: snes9x
```

User override in `batocera.conf`:
```ini
snes.core=bsnes
snes["mygame.sfc"].core=snes9x_next
```

Core file: `/usr/lib/libretro/{core}_libretro.so`
Core info: `/usr/share/libretro/info/{core}_libretro.info`

### Config Files Written

| File | Writer | Content |
|------|--------|---------|
| `retroarchcustom.cfg` | `libretroRetroarchCustom` | Base config (menu, input defaults, paths) |
| `retroarchcustom.cfg` | `libretroControllers` | Controller mappings (appended) |
| `retroarchcustom.cfg` | `libretroConfig` | All runtime settings (~800+ lines of logic) |
| `retroarch-core-options.cfg` | `libretroOptions` | Per-core options for ~50+ cores |

All written to `/userdata/system/configs/retroarch/`.

### libretroConfig Settings Categories

**Video:** `video_driver` (gl/glcore/vulkan), `video_fullscreen`, resolution,
`video_rotation`, `video_threaded`, `vrr_runloop_enable`, `video_black_frame_insertion`,
`video_smooth`, aspect ratio settings

**Audio:** `audio_driver` (pulse), `audio_latency` (64), `audio_volume`

**Paths:** `savestate_directory`, `savefile_directory`, `system_directory`,
`cache_directory`, `libretro_directory`, `libretro_info_path`

**Input:** `input_joypad_driver` (udev), `input_max_users` (16), per-player
device types, analog dpad modes

**Per-core device types:** Extensive lookup tables (`coreToP1Device`, `systemToP1Device`)
for systems needing non-standard libretro device types (keyboards for computers, etc.)

### Controller Mapping (libretroControllers.py)

ES button names → RetroArch names:
```python
retroarchbtns = {
    'a': 'a', 'b': 'b', 'x': 'x', 'y': 'y',
    'pageup': 'l', 'pagedown': 'r',
    'l2': 'l2', 'r2': 'r2', 'l3': 'l3', 'r3': 'r3',
    'start': 'start', 'select': 'select'
}
```

Per-system adaptations:
- **N64:** if no R2 trigger, remap L2 to L shoulder
- **Dreamcast:** shoulder button remapping
- **Fightstick layout:** swaps shoulder buttons

Hotkeys (forced, keyboard-based via hotkeygen):
```
hotkey + select = exit
hotkey + start = menu
hotkey + l1 = screenshot
hotkey + x = save state
hotkey + y = load state
```

Joystick axes (handle +/- split):
```
input_player{N}_{axis}_minus_axis = -{id}
input_player{N}_{axis}_plus_axis = +{id}
```

### Core Options (libretroOptions.py)

Per-core option blocks for ~50+ cores. Pattern:

```python
def _cap32_options(coreSettings, system, rom, guns, wheels):
    _set(coreSettings, 'cap32_combokey', 'y')
    _set_from_system(coreSettings, 'cap32_model', system, default='6128')
```

`_set()` writes a value directly.
`_set_from_system()` reads from `system.config` with a fallback default.

Written to: `/userdata/system/configs/retroarch/cores/retroarch-core-options.cfg`

### Shaders

1. Shader set from `shaderset` config (default: `sharp-bilinear-simple`)
2. Rendering defaults from `rendering-defaults.yml`
3. File selection: `.slangp` for Vulkan/glcore, `.glslp` for GL
4. Search: `/userdata/shaders/` then `/usr/share/batocera/shaders/`
5. Passed via: `--set-shader /path/to/shader.slangp`
6. If filename contains "noBezel", bezels are disabled

### Bezels

RetroArch handles bezels internally (`supportsInternalBezels() = True`):
- `.info` JSON files define bezel dimensions and margins
- Ratio validation (screen vs bezel compatibility)
- Can add player info "tattoo" and QR codes for RetroAchievements
- Uses RetroArch's overlay system

### RetroAchievements

~40 cores support cheevos:
```python
coreToRetroachievements = {'arduous', 'beetle-saturn', 'blastem', ...}
```

When enabled, writes:
```
cheevos_enable = true
cheevos_username = <from batocera.conf>
cheevos_password = <from batocera.conf>
cheevos_hardcore_mode_enable = <from config>
cheevos_leaderboards_enable = <from config>
```

### Command Assembly

```python
commandArray = [
    RETROARCH_BIN,
    "-L", "/usr/lib/libretro/{core}_libretro.so",
    "--config", "/userdata/system/configs/retroarch/retroarchcustom.cfg",
    "--set-shader", "/path/to/shader.slangp",   # if shader enabled
    "--verbose",
    rom_path,
]
# + appendconfig overlays for per-system settings
# + --host/--connect for netplay
# + --subsystem for special systems (neogeocd, gb_link, etc.)
```

Special system handling:
- **neogeocd:** `--subsystem neocd`
- **GB link:** `--subsystem gb_link_2p`
- **DOS:** `.dos`/`.pc` extension detection
- **Quake:** `pak0.pak` resolution
- **MAME:** `.cmd` file parsing

---

## Standalone Emulator Generators

### Common Pattern

Every standalone generator follows:
1. Create config directories
2. Write/update config files (INI, XML, JSON — format varies)
3. Write controller mappings (separate `{emulator}Controllers.py`)
4. Write `SDL_GAMECONTROLLERCONFIG` env var
5. Return `Command(array=[binary, ...args], env={...})`

### Dolphin (Wii/GameCube)

| Config file | Format | Content |
|-------------|--------|---------|
| `dolphin.ini` | INI | Main settings |
| `GFX.ini` | INI | Graphics backend, resolution |
| `GCPadNew.ini` | INI | GameCube controller mapping |
| `WiimoteNew.ini` | INI | Wii remote mapping |
| `SYSCONF` | Binary | Wii system settings |
| `Qt.ini` | INI | UI settings |

Command: `/usr/bin/dolphin-emu -e <rom> -b`
Backend: OGL or Vulkan (with fallback)
Supports: per-port controller type config (standard, GC adapter, various Wiimote types)

### PCSX2 (PS2)

Config: INI at `/userdata/system/configs/PCSX2/inis/`
SDL game controller DB written to env
Wheel support with physical wheel type detection (DrivingForce, GT Force)
State: `-statefile` and `-stateindex` args
Command: `/usr/pcsx2/bin/pcsx2-qt -nogui <rom>`

### PPSSPP (PSP)

Config: INI via `ppssppConfig.writePPSSPPConfig()`
Controllers: `ppssppControllers.generateControllerConfig()` → `controls.ini`
DPI adjustment for low-res screens: `--dpi 0.5`
Command: `/usr/bin/PPSSPP <rom> --fullscreen`

### Cemu (Wii U)

Config: `settings.xml` (XML via Python minidom)
Controllers: `cemuControllers.py` writes profile XMLs
Vulkan GPU selection with UUID
ROM resolution: finds `.rpx` inside squashfs directories
Command: `/usr/bin/cemu/cemu -f -g <rom> --force-no-menubar`

---

## Controller/Input Pipeline

### Data Flow

```
es_input.cfg (XML)
    ↓
Controller.load_for_players()
    ↓
Controller objects (per player)
    ↓
generator.generate()
    ↓
Emulator-specific config files
```

### Controller Object

```
Controller:
  name: str                    Device name from es_input.cfg
  type: "keyboard" | "joystick"
  guid: str                    SDL2 GUID
  player_number: int           1-based
  index: int                   Device index
  real_name: str               Actual device name
  device_path: str             /dev/input/eventX
  button_count: int
  hat_count: int
  axis_count: int
  inputs: dict[str, Input]     All mapped inputs
```

### Input Object

```
Input:
  name: str     "a", "b", "joystick1up", "up", "hotkey", etc.
  type: str     "button", "axis", "hat", "key"
  id: str       Button/axis/hat ID number
  value: str    Direction for axes (-1/1), hat value (1/2/4/8)
  code: str     Optional evdev code
```

### ES Standard Input Names

```
Face:     a, b, x, y
Shoulder: pageup (L1), pagedown (R1)
Trigger:  l2 (L2), r2 (R2)
Stick:    l3, r3
System:   start, select, hotkey
D-pad:    up, down, left, right
Sticks:   joystick1up, joystick1left, joystick2up, joystick2left
```

### SDL Game Controller DB

Each controller generates an SDL mapping line:
```
GUID,Name,platform:Linux,a:b0,b:b1,x:b2,y:b3,...
```

Written to `SDL_GAMECONTROLLERCONFIG` env var. Consumed by SDL2-based emulators.

### Hotkey System

`hotkeygen` daemon translates gamepad hotkey+button combos into keyboard events.
Each generator declares which keyboard combos map to which actions:

```
Generator declares: exit = [KEY_LEFTSHIFT, KEY_ESC]
Hotkeygen translates: hotkey+select on pad → Shift+Escape keypress
Emulator receives: Shift+Escape → exits
```

### Evmapy (Pad-to-Keyboard)

For emulators without native gamepad support, `batocera-evmapy` maps pad events
to keyboard events. Managed as a context manager (start on entry, stop on exit).

---

## Special Input Devices

### Lightguns

Detection via pyudev+evdev:
- Scan `/dev/input/event*` for `ID_INPUT_MOUSE=1` + `ID_INPUT_GUN=1`
- Read `ID_INPUT_GUN_NEED_CROSS` and `ID_INPUT_GUN_NEED_BORDERS`
- Enumerate mouse buttons via evdev capabilities

Precalibration: for arcade systems (atomiswave, naomi, mame, model2),
copy NVRAM files from `/usr/share/batocera/guns-precalibrations/`.

Gun borders: drawn around screen edges when `needs_borders=True`.
Size configurable: small/medium/large.

### Steering Wheels

`wheelsUtils.py` context manager:
1. Detect wheels via `ID_INPUT_WHEEL` udev property
2. Read `WHEEL_ROTATION_ANGLE` from udev
3. Remap buttons per-system (e.g., PS2: cross=b, square=y)
4. Adjust wheel range via sysfs `range` file
5. For range/deadzone adjustments, spawn `evsieve` virtual devices
6. Game-specific metadata from `wheelgames.xml`

### Controller Hot-Plug

Background monitor thread:
- `pyudev.Monitor` filtered on `input` subsystem
- On change: re-scan via pysdl2 for updated GUIDs and paths
- Revive disconnected controllers by GUID match
- Trigger evmapy reconfiguration

---

## Video Mode and Resolution

All operations through `batocera-resolution` CLI:

| Function | Command |
|----------|---------|
| Switch mode | `batocera-resolution setMode <mode>` |
| Get current mode | `batocera-resolution currentMode` |
| Get resolution | `batocera-resolution currentResolution` → "WxH" |
| Optimize resolution | `batocera-resolution minTomaxResolution` |
| Get refresh rate | `batocera-resolution refreshRate` |
| List outputs | `batocera-resolution listOutputs` |
| Show/hide cursor | `batocera-resolution changeMouse <0|1>` |

### Resolution Flow

1. If `videomode` is "default": call `minTomaxResolution()` (e.g., 4K → 1080p)
2. If generator requests specific mode: switch to it
3. After emulator exits: restore original mode

### Multi-Screen

`getScreensInfos()` detects up to 3 screens with position/size info.
Used by two-screen emulators (DS on two monitors, etc.).

---

## Error Handling

### Exception Hierarchy

```
BaseBatoceraException (exit_code=1)
├── BatoceraException (exit_code=250, writes message to /tmp/launch_error.log)
├── UnexpectedEmulatorExit (200)
├── BadCommandLineArguments (201)
├── InvalidConfiguration (202)
├── UnknownEmulator (203)
├── MissingEmulator (204)
└── MissingCore (205)
```

ES reads `/tmp/launch_error.log` to display errors to the user.

### Logging

- Format: `%(asctime)s %(levelname)s (%(filename)s:%(lineno)d):%(funcName)s %(message)s`
- DEBUG..INFO → stdout
- WARNING..CRITICAL → stderr
- `BrokenPipeError` silently swallowed (ES may truncate pipe)
- Emulator stdout/stderr captured via `subprocess.PIPE`, logged at DEBUG/ERROR

---

## Key Data Structures

| Type | Purpose |
|------|---------|
| `Command` | `{ array: [str], env: {str: str} }` — process to launch |
| `Emulator` | System name + resolved `SystemConfig` + game info |
| `SystemConfig` | Flat dict with typed getters (`.get_bool()`, `.emulator`, `.core`) |
| `Controller` | Player controller with full input mapping |
| `Input` | Single input (button/axis/hat) with name, type, id, value, code |
| `Gun` | Lightgun device with node, mouse_index, capabilities |
| `Resolution` | `{ width: int, height: int }` |
| `HotkeysContext` | `{ name: str, keys: {action: [key_combo]} }` |
| `DeviceInfo` | Udev device info (eventId, path, isWheel/Mouse/Joystick) |
| `BezelInfo` | Bezel dimensions and margins |
| `ScreenInfo` | Screen position and size |

---

## Rust Rewrite Architecture

### Module Mapping

| Python | Rust |
|--------|------|
| `Generator` ABC | `trait Generator` with default methods |
| `_GENERATOR_MAP` dict | `HashMap<String, Box<dyn Generator>>` or enum |
| `lazy import_module` | Feature flags or `inventory` crate |
| `Command` dataclass | `struct Command { args: Vec<OsString>, env: HashMap<String, OsString> }` |
| `SystemConfig` dict | `struct SystemConfig(HashMap<String, ConfigValue>)` with typed accessors |
| `UnixSettings` | Custom parser for `loisto.conf` (no section headers, `.` namespacing) |
| `subprocess.Popen` | `std::process::Command` |
| `pyudev` | `udev` crate |
| `evdev` | `evdev` crate |
| `pysdl2` | `sdl2` crate |
| Context managers | RAII types with `Drop` |
| YAML defaults | `serde_yaml` |
| XML parsing | `quick-xml` + `serde` |
| INI writing | `configparser` crate or custom |
| Signal handling | `signal-hook` crate |

### Generator Trait

```rust
pub trait Generator {
    /// Write config files, return the command to launch
    fn generate(
        &self,
        system: &Emulator,
        rom: &Path,
        controllers: &[Controller],
        metadata: &GameMetadata,
        guns: &[Gun],
        wheels: &HashMap<String, DeviceInfo>,
        resolution: Resolution,
    ) -> Result<Command>;

    /// Hotkey definitions for this emulator
    fn hotkeys_context(&self) -> HotkeysContext;

    /// Optional overrides
    fn resolution_mode(&self, config: &SystemConfig) -> &str {
        config.get_str("videomode", "default")
    }
    fn mouse_mode(&self, _config: &SystemConfig, _rom: &Path) -> bool { false }
    fn execution_directory(&self, _config: &SystemConfig, _rom: &Path) -> Option<PathBuf> { None }
    fn supports_internal_bezels(&self) -> bool { false }
    fn in_game_ratio(&self, _config: &SystemConfig, _res: Resolution, _rom: &Path) -> f64 { 4.0/3.0 }
}
```

### Config Cascade in Rust

```rust
pub struct ConfigCascade {
    defaults: HashMap<String, HashMap<String, String>>,      // from YAML
    arch_defaults: HashMap<String, HashMap<String, String>>,  // from arch YAML
    user_config: UserConf,                                      // from loisto.conf
}

impl ConfigCascade {
    pub fn resolve(&self, system: &str, rom: &str, folder: &str) -> SystemConfig {
        let mut config = HashMap::new();

        // 1. Base defaults for this system
        if let Some(defaults) = self.defaults.get(system) {
            config.extend(defaults.clone());
        }

        // 2. Arch defaults
        if let Some(arch) = self.arch_defaults.get(system) {
            config.extend(arch.clone());
        }

        // 3. Global user settings
        config.extend(self.user_config.get_global());

        // 4. Per-system user settings
        config.extend(self.user_config.get_system(system));

        // 5. Per-folder user settings
        config.extend(self.user_config.get_folder(system, folder));

        // 6. Per-game user settings
        config.extend(self.user_config.get_game(system, rom));

        SystemConfig(config)
    }
}
```

### Config File Parser

Batocera uses `batocera.conf`; loisto uses `loisto.conf` at `/data/system/loisto.conf`.
The format is non-standard INI (no section headers, dot-namespaced keys with bracket
indexing for per-game overrides). Needs a custom parser:

```rust
pub struct UserConf {
    entries: Vec<(String, String)>,  // preserve order for writing back
}

impl UserConf {
    pub fn get_global(&self) -> HashMap<String, String> {
        // Keys matching: global.{key} → {key}
    }

    pub fn get_system(&self, system: &str) -> HashMap<String, String> {
        // Keys matching: {system}.{key} → {key}
        // Exclude bracket-indexed keys
    }

    pub fn get_folder(&self, system: &str, folder: &str) -> HashMap<String, String> {
        // Keys matching: {system}.folder["{folder}"].{key} → {key}
    }

    pub fn get_game(&self, system: &str, game: &str) -> HashMap<String, String> {
        // Keys matching: {system}["{game}"].{key} → {key}
    }
}
```

### Data-Driven Config Generation

The Python codebase has ~800+ lines of conditionals in `libretroConfig.py` and
similar patterns in every standalone generator. A Rust rewrite can make this more
data-driven:

```toml
# retroarch-settings.toml

[video]
video_driver = { default = "gl", vulkan_if = "video.vulkan" }
video_fullscreen = "true"
video_smooth = { from_config = "smooth", default = "false" }
video_rotation = { from_config = "rotation", default = "0" }

[audio]
audio_driver = "pulse"
audio_latency = { from_config = "audio_latency", default = "64" }

[per_core.snes9x]
snes9x_overscan = { from_config = "overscan", default = "enabled" }

[per_system.n64.device_types]
p1_device = "5"  # analog controller
```

This trades Python code for declarative TOML tables — easier to maintain and validate.

### Generator Registration

```rust
// Option A: Static dispatch via enum
pub enum EmulatorKind {
    Libretro(LibretroGenerator),
    Dolphin(DolphinGenerator),
    Pcsx2(Pcsx2Generator),
    // ...
}

// Option B: Dynamic dispatch via registry
pub fn get_generator(name: &str) -> Option<Box<dyn Generator>> {
    match name {
        "libretro" => Some(Box::new(LibretroGenerator)),
        "dolphin" => Some(Box::new(DolphinGenerator)),
        "pcsx2" => Some(Box::new(Pcsx2Generator)),
        // ...
        _ => None,
    }
}

// Option C: Plugin system via inventory crate
inventory::submit! { GeneratorEntry("libretro", || Box::new(LibretroGenerator)) }
inventory::submit! { GeneratorEntry("dolphin", || Box::new(DolphinGenerator)) }
```

### Estimated Module Breakdown

| Module | LOC (est.) | Notes |
|--------|-----------|-------|
| `main.rs` | ~300 | CLI parsing, orchestration flow |
| `config.rs` | ~400 | Config cascade, batocera.conf parser, SystemConfig |
| `controller.rs` | ~300 | Controller/Input types, es_input.cfg parser, SDL DB |
| `emulator.rs` | ~200 | Emulator type, defaults loading |
| `command.rs` | ~50 | Command struct |
| `video.rs` | ~100 | Resolution switching (gamescope handles this natively) |
| `hotkeys.rs` | ~100 | Hotkeygen context management |
| `guns.rs` | ~200 | Gun detection, precalibration |
| `wheels.rs` | ~200 | Wheel detection, remapping |
| `bezels.rs` | ~200 | Bezel selection and manipulation |
| `generators/libretro.rs` | ~800 | RetroArch config writing |
| `generators/libretro_options.rs` | ~600 | Per-core options (data-driven) |
| `generators/libretro_controllers.rs` | ~300 | Controller mapping to RA format |
| `generators/dolphin.rs` | ~300 | Dolphin config+controllers |
| `generators/pcsx2.rs` | ~250 | PCSX2 config+controllers |
| `generators/ppsspp.rs` | ~200 | PPSSPP config+controllers |
| `generators/standalone_common.rs` | ~200 | Shared helpers for standalone gens |
| `generators/...` | ~100 each | Other standalone generators |
| **Total (core + ~10 generators)** | **~5000** | Excluding data files |

### Key Simplifications in Rust

1. **Data-driven configs** — TOML tables instead of 800 lines of Python conditionals
2. **RAII cleanup** — Drop traits replace context managers, no `try/finally` needed
3. **Type safety** — `SystemConfig` with typed accessors catches mismatches at compile time
4. **No GIL** — controller monitor thread can share state without Python's threading overhead
5. **Single binary** — no Python interpreter, no pip dependencies, no version conflicts
6. **Config validation** — serde deserialization catches malformed YAML/TOML/INI at load time

### What Stays as Subprocess Calls

These are external tools that stay as-is:
- `batocera-resolution` — display mode switching (**needs replacement:** gamescope handles resolution and display mode switching natively)
- `hotkeygen` — hotkey daemon
- `batocera-evmapy` — pad-to-keyboard mapping (**needs replacement:** our overlay or configgen handles input mapping directly)
- `evsieve` — virtual input device creation
- `mount`/`umount` — SquashFS handling

---

## Filesystem Paths

### Batocera reference (batoceraPaths.py)

| Constant | Batocera Path |
|----------|---------------|
| `CONF` | `/userdata/system/batocera.conf` |
| `SAVES` | `/userdata/saves` |
| `ROMS` | `/userdata/roms` |
| `BIOS` | `/userdata/bios` |
| `CONFIGS` | `/userdata/system/configs` |
| `OVERLAYS` | `/userdata/overlays` |
| `SHADERS` | `/usr/share/batocera/shaders` |
| `USER_SHADERS` | `/userdata/shaders` |
| `CORES` | `/usr/lib/libretro` |
| `CORE_INFO` | `/usr/share/libretro/info` |
| `SCREENSHOTS` | `/userdata/screenshots` |
| `DECORATIONS` | `/usr/share/batocera/datainit/decorations` |
| `EVMAPY` | `/userdata/system/configs/evmapy` |
| `LOGS` | `/userdata/system/logs` |

### Loisto paths

| Constant | Loisto Path |
|----------|-------------|
| `CONF` | `/data/system/loisto.conf` |
| `SAVES` | `/data/saves` |
| `ROMS` | `/data/roms` |
| `BIOS` | `/data/bios` |
| `CONFIGS` | `/data/system/configs` |
| `OVERLAYS` | `/data/overlays` |
| `SHADERS` | `/usr/share/loisto/shaders` |
| `USER_SHADERS` | `/data/shaders` |
| `CORES` | `/usr/lib/libretro` |
| `CORE_INFO` | `/usr/share/libretro/info` |
| `SCREENSHOTS` | `/data/screenshots` |
| `LOGS` | `/data/system/logs` |

---

## Sources

- [batocera-configgen source](https://github.com/batocera-linux/batocera.linux/tree/master/package/batocera/core/batocera-configgen)
- [emulatorlauncher.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/emulatorlauncher.py)
- [Generator.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/generators/Generator.py)
- [libretroGenerator.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/generators/libretro/libretroGenerator.py)
- [libretroConfig.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/generators/libretro/libretroConfig.py)
- [libretroControllers.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/generators/libretro/libretroControllers.py)
- [libretroOptions.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/generators/libretro/libretroOptions.py)
- [controller.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/controller.py)
- [Emulator.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/Emulator.py)
- [batoceraPaths.py](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/core/batocera-configgen/configgen/configgen/batoceraPaths.py)
- [Batocera Wiki](https://wiki.batocera.org/)
