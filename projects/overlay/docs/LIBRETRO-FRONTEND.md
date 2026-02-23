# Libretro Frontend in Rust — Research

## TL;DR

Building a custom libretro frontend in Rust is viable and surprisingly small (~2000-4000 LOC).
The libretro API is ~15 functions to call + 6 callbacks to implement. Key enablers: `librashader`
(Rust crate, wgpu runtime) eliminates shader pipeline work, `rcheevos` C FFI gives native
achievement evaluation with direct memory access, and cores are standard `.so` files at
`/usr/lib/libretro/`. RetroArch becomes optional middleware we don't need.

---

## Why Drop RetroArch

| RetroArch provides | Loisto alternative | Notes |
|--------------------|--------------------|-------|
| Core loading + callbacks | `libloading` + `libretro-sys` | ~500 LOC |
| Video output | wgpu (already in stack) | Upload pixel buffer as texture |
| Audio output | `cpal` crate | ~200 LOC |
| Input mapping | SDL2 / evdev (already in stack) | ~200 LOC |
| Shader pipeline | `librashader` crate (wgpu runtime) | Drop-in, production-ready |
| RetroAchievements | `rcheevos` C FFI (rc_client) | Direct memory access from core |
| Save states | `retro_serialize` + file I/O | ~100 LOC |
| Core options | HashMap + config file | ~100 LOC |
| Rewind | Ring buffer of save states | ~400 LOC, deferrable |
| Fast-forward | Skip frame limiter | ~10 LOC |
| Run-ahead | Two-instance trick | ~800 LOC, deferrable |
| Netplay | Skip entirely | Thousands of LOC, not needed |
| Menu system | romhoard kiosk UI | Already exists |

**What we gain:**
- Single-process architecture (no RetroArch window to manage under gamescope)
- Native achievement toasts rendered by vello overlay
- Direct control over frame pacing, input latency, audio sync
- Shader pipeline through our own wgpu surface
- No RetroArch menu system fighting our UI
- Smaller attack surface, fewer moving parts

**What we lose:**
- RetroArch's 10+ years of core quirk workarounds
- Massive input autoconfig database (can copy the text files)
- Netplay (don't need it)
- Community support / debugging ecosystem

---

## The Libretro API

### Functions to call on the core (~15)

```
retro_set_environment()           — register environment callback (MUST be first)
retro_set_video_refresh()         — register video frame callback
retro_set_audio_sample()          — register single-sample audio callback
retro_set_audio_sample_batch()    — register batch audio callback (preferred)
retro_set_input_poll()            — register input polling callback
retro_set_input_state()           — register input state query callback
retro_init()                      — initialize the core
retro_get_system_info()           — query core metadata
retro_get_system_av_info()        — get geometry + timing (resolution, FPS, sample rate)
retro_load_game()                 — load ROM
retro_run()                       — execute exactly one frame
retro_unload_game()               — unload content
retro_deinit()                    — shut down
retro_serialize() / unserialize() — save/load state
retro_serialize_size()            — query save state buffer size
retro_get_memory_data()           — direct pointer to emulated RAM
retro_get_memory_size()           — size of memory region
retro_set_controller_port_device()— configure controller types
```

### Callbacks the frontend provides (6)

| Callback | Signature | When |
|----------|-----------|------|
| `video_refresh` | `(data, width, height, pitch)` | Once per `retro_run()` |
| `audio_sample_batch` | `(data, frames) -> size_t` | Once per `retro_run()` |
| `audio_sample` | `(left, right)` | Per-sample (alternative) |
| `input_poll` | `()` | At least once per `retro_run()` |
| `input_state` | `(port, device, index, id) -> int16_t` | Core queries specific buttons |
| `environment` | `(cmd, data) -> bool` | Core queries frontend capabilities |

### The environment callback

The `retro_environment_t` callback has 52+ possible commands. Most are optional (return `false`
for unsupported). Minimum viable set:

**Must have:**
- `GET_CAN_DUPE` → return `true`
- `SET_PIXEL_FORMAT` → accept XRGB8888 or RGB565
- `GET_SYSTEM_DIRECTORY` → return a path
- `GET_SAVE_DIRECTORY` → return a path
- `GET_LOG_INTERFACE` → provide a logger

**Needed for real cores:**
- `SET_VARIABLES` / `GET_VARIABLE` / `GET_VARIABLE_UPDATE` → core options
- `SET_INPUT_DESCRIPTORS` → input labeling
- `SET_HW_RENDER` → GPU-rendered cores (PS1, N64, PSP)
- `SET_GEOMETRY` / `SET_SYSTEM_AV_INFO` → runtime resolution changes
- `GET_RUMBLE_INTERFACE` → controller vibration
- `SET_DISK_CONTROL_INTERFACE` → multi-disc games

---

## Existing Minimal Frontends (reference)

| Project | Language | LOC | Notes |
|---------|----------|-----|-------|
| [nanoarch](https://github.com/heuripedes/nanoarch) | C | ~600 | Canonical minimal example. GLFW+OpenGL+ALSA. |
| [sdlarch](https://github.com/heuripedes/sdlarch) | C | ~1000 | SDL2 variant of nanoarch |
| [miniretro](https://github.com/davidgfnet/miniretro) | C/C++ | ~2000 | CLI tool for core testing, frame dumping |
| [Ludo](https://ludo.libretro.com) | Go | ~15k | Full GUI frontend, maintained by libretro org |
| [go-nanoarch](https://github.com/libretro/go-nanoarch) | Go | ~1000 | Go port of nanoarch, libretro-maintained |
| [RustroArch tutorial](https://www.retroreversing.com/CreateALibRetroFrontEndInRust) | Rust | ~1500 | Tutorial using `libretro-sys` + `minifb` + `rodio` |
| [picoarch](https://docs.libretro.com/development/frontends/) | C | Small | Targets low-power ARM devices |

**Key takeaway:** A bare-minimum frontend is ~600 LOC. A usable one is 2000-5000. Ludo (full GUI)
is ~15k — still 30x smaller than RetroArch (~500k).

---

## Rust Crates

### Core loading

| Crate | Version | Purpose |
|-------|---------|---------|
| `libretro-sys` | 0.1.1 | Hand-written FFI types for `libretro.h`. Simple, sufficient for a frontend. |
| `rust-libretro-sys` | 0.3.2 | Bindgen-generated, more complete. Part of larger rust-libretro ecosystem. |
| `libloading` | 0.9.0 | `dlopen` + symbol resolution. |

`libretro-sys` is simpler and what the RustroArch tutorial uses. May be missing newer
environment constants — can vendor and extend.

### Core loading pattern

```rust
use libloading::{Library, Symbol};

pub struct LibretroCore {
    _lib: Library,  // must outlive function pointers
    pub retro_init: unsafe extern "C" fn(),
    pub retro_deinit: unsafe extern "C" fn(),
    pub retro_run: unsafe extern "C" fn(),
    pub retro_load_game: unsafe extern "C" fn(*const GameInfo) -> bool,
    pub retro_unload_game: unsafe extern "C" fn(),
    pub retro_get_system_info: unsafe extern "C" fn(*mut SystemInfo),
    pub retro_get_system_av_info: unsafe extern "C" fn(*mut SystemAvInfo),
    pub retro_set_environment: unsafe extern "C" fn(EnvironmentFn),
    pub retro_set_video_refresh: unsafe extern "C" fn(VideoRefreshFn),
    pub retro_set_audio_sample_batch: unsafe extern "C" fn(AudioSampleBatchFn),
    pub retro_set_input_poll: unsafe extern "C" fn(InputPollFn),
    pub retro_set_input_state: unsafe extern "C" fn(InputStateFn),
    pub retro_serialize_size: unsafe extern "C" fn() -> usize,
    pub retro_serialize: unsafe extern "C" fn(*mut c_void, usize) -> bool,
    pub retro_unserialize: unsafe extern "C" fn(*const c_void, usize) -> bool,
    pub retro_get_memory_data: unsafe extern "C" fn(c_uint) -> *mut c_void,
    pub retro_get_memory_size: unsafe extern "C" fn(c_uint) -> usize,
}

impl LibretroCore {
    pub unsafe fn load(path: &str) -> Result<Self> {
        let lib = Library::new(path)?;
        macro_rules! sym {
            ($name:ident, $ty:ty) => {
                *lib.get::<$ty>(stringify!($name).as_bytes())?
            };
        }
        Ok(Self {
            retro_init: sym!(retro_init, unsafe extern "C" fn()),
            retro_run: sym!(retro_run, unsafe extern "C" fn()),
            retro_load_game: sym!(retro_load_game, unsafe extern "C" fn(*const GameInfo) -> bool),
            // ... etc for all symbols
            _lib: lib,
        })
    }
}
```

**Safety:** `Library` must outlive all extracted function pointers. Keep `_lib` in the struct.

### Shaders — librashader

| Crate | Version | Purpose |
|-------|---------|---------|
| `librashader` | 0.10.1 | Full RetroArch slang shader pipeline |

Runtimes: OpenGL, Vulkan, Metal, D3D11/12, **wgpu**. Feature flag: `wgpu`.

Used in production by the Ares emulator. Parses `.slangp` presets, handles multi-pass
rendering, LUT textures, frame history. The wgpu runtime has one known limitation:
no FSR shaders (blocked on ImageGather in wgpu/naga).

```rust
use librashader::presets::ShaderPreset;
use librashader::runtime::wgpu::FilterChain;

// Parse a .slangp preset (e.g., CRT-Royale, NTSC-Adaptive)
let preset = ShaderPreset::try_parse("shaders/crt-royale.slangp")?;

// Create filter chain with wgpu device
let filter_chain = FilterChain::load_from_preset(
    preset, &device, &queue, None,
)?;

// Each frame: pass core framebuffer through shader chain
filter_chain.frame(
    &input_texture,   // game framebuffer as wgpu texture
    &viewport,        // output dimensions
    frame_count,
    &command_encoder,
    &output_view,     // final render target
    None,
)?;
```

RetroArch shader presets (`.slangp` files) work directly with librashader. Loisto
ships these at `/usr/share/loisto/shaders/`.

### Audio — cpal

| Crate | Version | Purpose |
|-------|---------|---------|
| `cpal` | 0.17.3 | Cross-platform audio output |

Backends: ALSA (Linux, works through PipeWire's ALSA compat layer), CoreAudio (macOS),
WASAPI (Windows). No native PipeWire backend but transparent via `pipewire-alsa`.

Note: loisto runs PipeWire standalone without WirePlumber. The `pipewire-alsa` compatibility
layer should work for simple playback (which is all we need for core audio output), but
this needs validation — session management features that depend on WirePlumber will not
be available.

Libretro cores output interleaved stereo `i16` PCM at the core's declared sample rate
(typically 32000-48000 Hz).

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

let host = cpal::default_host();
let device = host.default_output_device().unwrap();
let config = cpal::StreamConfig {
    channels: 2,
    sample_rate: cpal::SampleRate(48000),
    buffer_size: cpal::BufferSize::Default,
};

// Use a lock-free ring buffer (ringbuf crate) between emu thread and audio callback
let (mut producer, mut consumer) = ringbuf::HeapRb::<f32>::new(4096).split();

let stream = device.build_output_stream(
    &config,
    move |data: &mut [f32], _| {
        for sample in data.iter_mut() {
            *sample = consumer.pop().unwrap_or(0.0);
        }
    },
    |err| eprintln!("audio error: {err}"),
    None,
)?;
stream.play()?;

// In audio_sample_batch callback: convert i16 → f32, push to ring buffer
fn push_audio(samples: &[i16], producer: &mut Producer<f32>) {
    for &s in samples {
        let _ = producer.push(s as f32 / 32768.0);
    }
}
```

For quality resampling (core sample rate → device sample rate), use the `rubato` crate
or `samplerate` (bindings to libsamplerate).

### RetroAchievements — rcheevos C FFI

| Dependency | Purpose |
|------------|---------|
| `cc` (build) | Compile rcheevos C sources |
| `bindgen` (build) | Generate Rust bindings from C headers |

rcheevos is pure C with no external dependencies. Vendor it as a git submodule.

**Headers needed:**
- `rc_client.h` — main API (modern integration point)
- `rc_hash.h` — ROM hashing for game identification
- `rc_consoles.h` — console ID constants

**build.rs:**

```rust
fn main() {
    cc::Build::new()
        .include("vendor/rcheevos/include")
        .include("vendor/rcheevos/src")
        // Core client
        .file("vendor/rcheevos/src/rc_client.c")
        .file("vendor/rcheevos/src/rc_compat.c")
        .file("vendor/rcheevos/src/rc_util.c")
        .file("vendor/rcheevos/src/rc_version.c")
        .file("vendor/rcheevos/src/rc_libretro.c")
        // Achievement engine
        .file("vendor/rcheevos/src/rcheevos/alloc.c")
        .file("vendor/rcheevos/src/rcheevos/condition.c")
        .file("vendor/rcheevos/src/rcheevos/condset.c")
        .file("vendor/rcheevos/src/rcheevos/consoleinfo.c")
        .file("vendor/rcheevos/src/rcheevos/format.c")
        .file("vendor/rcheevos/src/rcheevos/lboard.c")
        .file("vendor/rcheevos/src/rcheevos/memref.c")
        .file("vendor/rcheevos/src/rcheevos/operand.c")
        .file("vendor/rcheevos/src/rcheevos/rc_validate.c")
        .file("vendor/rcheevos/src/rcheevos/richpresence.c")
        .file("vendor/rcheevos/src/rcheevos/runtime.c")
        .file("vendor/rcheevos/src/rcheevos/runtime_progress.c")
        .file("vendor/rcheevos/src/rcheevos/trigger.c")
        .file("vendor/rcheevos/src/rcheevos/value.c")
        // API layer
        .file("vendor/rcheevos/src/rapi/rc_api_common.c")
        .file("vendor/rcheevos/src/rapi/rc_api_editor.c")
        .file("vendor/rcheevos/src/rapi/rc_api_info.c")
        .file("vendor/rcheevos/src/rapi/rc_api_runtime.c")
        .file("vendor/rcheevos/src/rapi/rc_api_user.c")
        // Hash
        .file("vendor/rcheevos/src/rhash/aes.c")
        .file("vendor/rcheevos/src/rhash/cdreader.c")
        .file("vendor/rcheevos/src/rhash/hash.c")
        .file("vendor/rcheevos/src/rhash/md5.c")
        .define("RC_DISABLE_LUA", None)
        .warnings(false)
        .compile("rcheevos");

    bindgen::Builder::default()
        .header("vendor/rcheevos/include/rc_client.h")
        .header("vendor/rcheevos/include/rc_hash.h")
        .header("vendor/rcheevos/include/rc_consoles.h")
        .clang_arg("-Ivendor/rcheevos/include")
        .allowlist_function("rc_.*")
        .allowlist_type("rc_.*")
        .allowlist_var("RC_.*")
        .generate()
        .expect("bindgen failed")
        .write_to_file(
            std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap())
                .join("rcheevos.rs")
        )
        .unwrap();
}
```

---

## rcheevos rc_client Integration

With direct memory access from `retro_get_memory_data()`, we can use the **full rc_client**
— not just rapi. This means native per-frame achievement evaluation inside loisto.

### Three callbacks to implement

**1. Memory reader** — reads emulated RAM for achievement condition evaluation:

```rust
unsafe extern "C" fn rc_read_memory(
    address: u32,
    buffer: *mut u8,
    num_bytes: u32,
    _client: *mut rc_client_t,
) -> u32 {
    let mem = (CORE.retro_get_memory_data)(RETRO_MEMORY_SYSTEM_RAM);
    let size = (CORE.retro_get_memory_size)(RETRO_MEMORY_SYSTEM_RAM);

    if mem.is_null() || (address as usize + num_bytes as usize) > size {
        return 0;
    }

    std::ptr::copy_nonoverlapping(
        mem.cast::<u8>().add(address as usize),
        buffer,
        num_bytes as usize,
    );
    num_bytes
}
```

**2. HTTP server callback** — rc_client builds request URLs, you perform HTTP:

```rust
unsafe extern "C" fn rc_server_call(
    request: *const rc_api_request_t,
    callback: rc_client_server_callback_t,
    callback_data: *mut c_void,
    _client: *mut rc_client_t,
) {
    let url = CStr::from_ptr((*request).url).to_str().unwrap();
    let post_data = if (*request).post_data.is_null() {
        None
    } else {
        Some(CStr::from_ptr((*request).post_data).to_str().unwrap())
    };

    // Spawn async HTTP request, call callback with response
    tokio::spawn(async move {
        let resp = reqwest::Client::new()
            .post(url)
            .body(post_data.unwrap_or("").to_string())
            .send().await;
        // ... marshal response back to callback
    });
}
```

**3. Event handler** — achievement triggers, leaderboard events, game completion:

```rust
unsafe extern "C" fn rc_event_handler(
    event: *const rc_client_event_t,
    _client: *mut rc_client_t,
) {
    match (*event).type_ {
        RC_CLIENT_EVENT_ACHIEVEMENT_TRIGGERED => {
            let achievement = (*event).achievement;
            let title = CStr::from_ptr((*achievement).title);
            let badge = CStr::from_ptr((*achievement).badge_name);
            // → Send to overlay: render toast with vello, show badge image
        }
        RC_CLIENT_EVENT_LEADERBOARD_STARTED => { /* show tracker */ }
        RC_CLIENT_EVENT_GAME_COMPLETED => { /* mastery notification */ }
        _ => {}
    }
}
```

### Lifecycle

```rust
// 1. Create
let client = rc_client_create(rc_read_memory, rc_server_call);
rc_client_set_event_handler(client, rc_event_handler);

// 2. Login (token from config)
rc_client_begin_login_with_token(client, username, token, login_callback, null);

// 3. Identify game (hashes ROM, fetches achievement definitions)
rc_client_begin_identify_and_load_game(
    client, console_id, rom_data, rom_size, load_callback, null,
);

// 4. Per frame (after retro_run)
rc_client_do_frame(client);

// 5. When paused
rc_client_idle(client);  // at least once per second

// 6. Save states — must serialize achievement progress too
let progress_size = rc_client_progress_size(client);
let mut progress = vec![0u8; progress_size];
rc_client_serialize_progress(client, progress.as_mut_ptr());
// Store alongside game save state

// 7. Hardcore enforcement
// When hardcore enabled: disable save state loading, rewind, cheats, slow-mo
```

### Validation requirement

Your frontend needs a unique User-Agent and must be **validated by RetroAchievements
admins** for hardcore unlocks. Without validation, unlocks get demoted to softcore.
Contact RA team at [retroachievements.org](https://retroachievements.org).

---

## Video Output

### Software-rendered cores (majority)

Core calls `video_refresh(data, width, height, pitch)` with a pixel buffer.
Pixel formats: `XRGB8888` (most common), `RGB565`, or `0RGB1555` (legacy).

For wgpu:
1. Receive pixel buffer in callback
2. Upload as `wgpu::Texture` via `queue.write_texture()`
3. Optionally pass through `librashader` filter chain
4. Render textured quad to display

### Hardware-rendered cores (PS1, N64, PSP)

Core requests `RETRO_ENVIRONMENT_SET_HW_RENDER` with a `retro_hw_render_callback`:
- `context_type`: `OPENGL`, `OPENGL_CORE`, or `VULKAN`
- `get_current_framebuffer`: frontend provides FBO handle for core to render into
- `get_proc_address`: core resolves GL function pointers through this
- `context_reset` / `context_destroy`: lifecycle callbacks

Frontend must create a shared GL/VK context. Core renders into the FBO, then calls
`video_refresh` with `RETRO_HW_FRAMEBUFFER_VALID` (special sentinel pointer).

**wgpu interop options:**

| Approach | Complexity | Performance |
|----------|-----------|-------------|
| Readback (glReadPixels → CPU → wgpu texture) | Low | GPU→CPU→GPU round trip per frame |
| `wgpu-hal` texture import (share GL/VK resources) | High | Zero-copy |
| Separate GL context + shared texture | Medium | Near zero-copy |

**Recommendation:** Start with readback for HW cores. Optimize later if needed.
Most retro cores are software-rendered anyway.

### Pixel format handling

```rust
fn upload_frame(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    data: *const c_void,
    width: u32,
    height: u32,
    pitch: usize,  // bytes per row, may include padding
    format: PixelFormat,
) {
    let bytes_per_pixel = match format {
        PixelFormat::XRGB8888 => 4,
        PixelFormat::RGB565 => 2,
    };

    // pitch may differ from width * bpp (row padding)
    let src = unsafe {
        std::slice::from_raw_parts(data as *const u8, height as usize * pitch)
    };

    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        src,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pitch as u32), rows_per_image: None },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}
```

---

## Frame Pacing & Audio Sync

### Audio-driven sync (recommended)

Standard approach from [RetroArch's Dynamic Rate Control paper](https://docs.libretro.com/development/cores/dynamic-rate-control/).

The core declares `fps` and `sample_rate` in `retro_system_av_info`. Each `retro_run()`
produces exactly `sample_rate / fps` audio samples and 1 video frame.

```
Audio buffer fill level acts as feedback:
  buffer filling up → emulator running fast → slow down resampling
  buffer draining   → emulator running slow → speed up resampling

ratio_new = ratio_old * (1.0 + k * (fill_level - target))
```

Where `k ≈ 0.005` and `target ≈ 0.5` (50% buffer fill).

### Main loop skeleton

```rust
loop {
    // 1. Poll input (SDL2 / evdev)
    poll_gamepads(&mut input_state);

    // 2. Run one emulated frame
    unsafe { (core.retro_run)(); }

    // 3. Achievement evaluation
    unsafe { rc_client_do_frame(rc); }

    // 4. Audio: resample and push to ring buffer
    let ratio = compute_resample_ratio(audio_buffer_fill);
    let resampled = resample(&pending_audio, ratio);
    audio_producer.push_slice(&resampled);

    // 5. Video: upload frame → shader chain → present
    if let Some(frame) = pending_frame.take() {
        upload_frame(&queue, &game_texture, &frame);
        shader_chain.frame(&game_texture, &viewport, frame_count, &encoder, &output);
        surface.present();
    }

    // 6. Throttle (if no vsync)
    if !vsync {
        let target = Duration::from_secs_f64(1.0 / target_fps);
        let elapsed = frame_start.elapsed();
        if elapsed < target {
            std::thread::sleep(target - elapsed);
        }
    }

    frame_count += 1;
}
```

### Resampling crates

| Crate | Type | Notes |
|-------|------|-------|
| `rubato` | Pure Rust | High quality, supports dynamic ratio changes |
| `samplerate` | C bindings (libsamplerate) | Industry standard, very high quality |

---

## Filesystem Layout

### Cores

Location: `/usr/lib/libretro/`
Examples: `snes9x_libretro.so`, `mgba_libretro.so`, `beetle_psx_hw_libretro.so`

Standard libretro ABI — loadable by any frontend via `dlopen`. Nothing RetroArch-specific.

User override path: `/data/system/configs/cores/`

### Shaders

Location: `/usr/share/loisto/shaders/`
Format: `.slangp` presets + `.slang` source files

These work directly with `librashader`.

### System files (BIOS)

Location: `/data/bios/`
Cores request these via `RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY`.

### Save data

Location: `/data/saves/` (SRAM) and `/data/states/` (save states)
The frontend manages these paths and passes them via environment callbacks.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                    loisto (Rust)                      │
│                                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐ │
│  │ core_loader  │  │ achievements│  │   overlay    │ │
│  │              │  │             │  │              │ │
│  │ libloading + │  │ rcheevos    │  │ vello+wgpu   │ │
│  │ libretro-sys │  │ rc_client   │  │ bars/toasts  │ │
│  │              │  │ (C FFI)     │  │ game menu    │ │
│  └──────┬───────┘  └──────┬──────┘  └──────┬───────┘ │
│         │                  │                │         │
│  ┌──────▼──────────────────▼────────────────▼───────┐ │
│  │                   main loop                       │ │
│  │  retro_run() → rc_client_do_frame() → render()   │ │
│  └──────┬───────────────────────────────────┬───────┘ │
│         │                                    │         │
│  ┌──────▼──────┐  ┌────────────┐  ┌────────▼───────┐ │
│  │   video     │  │   audio    │  │    input       │ │
│  │             │  │            │  │                │ │
│  │ wgpu texture│  │ cpal +     │  │ SDL2/evdev     │ │
│  │ librashader │  │ ring buffer│  │ gamepad map    │ │
│  └──────┬──────┘  └─────┬─────┘  └────────────────┘ │
│         │                │                            │
└─────────┼────────────────┼────────────────────────────┘
          │                │
   ┌──────▼──────┐  ┌─────▼─────┐
   │  gamescope  │  │ ALSA/     │
   │  (display)  │  │ PipeWire  │
   └─────────────┘  └───────────┘
```

### Module breakdown (estimated LOC)

| Module | Purpose | LOC |
|--------|---------|-----|
| `core_loader.rs` | dlopen, symbol resolution, environment callback | ~500 |
| `video.rs` | Pixel buffer upload, wgpu texture, librashader integration | ~300 |
| `audio.rs` | cpal output, ring buffer, dynamic resampling | ~300 |
| `input.rs` | SDL2/evdev → libretro button mapping, autoconfig | ~200 |
| `achievements.rs` | rcheevos FFI, memory reader, event handler, badge cache | ~500 |
| `saves.rs` | Save states, SRAM persistence, slot management | ~150 |
| `core_options.rs` | Key-value core option storage | ~100 |
| `config.rs` | Per-core and global configuration | ~200 |
| **Total core runtime** | | **~2250** |

Plus the existing overlay (vello bars, toasts, game menu) which is separate.

---

## Phased Implementation

### Phase 1: Software cores + basic playback

- Core loading via `libloading`
- Environment callback (minimum viable set)
- Video: pixel buffer → wgpu texture → screen
- Audio: cpal ring buffer with simple throttling
- Input: SDL2 GameController → libretro joypad
- Save states: serialize/unserialize + file I/O
- Core options: hashmap + TOML config
- **Target:** play SNES, Genesis, GBA, NES, arcade games

### Phase 2: Shaders + achievements

- `librashader` integration (wgpu runtime)
- rcheevos C FFI (rc_client + rhash)
- Badge image caching and overlay toasts
- Leaderboard display
- **Target:** CRT shaders, native achievement popups

### Phase 3: HW render cores

- `SET_HW_RENDER` with OpenGL context
- Readback approach (glReadPixels → wgpu)
- **Target:** PS1 (Beetle HW), N64 (Mupen64Plus-Next), PSP (PPSSPP)

### Phase 4: Quality of life

- Rewind (ring buffer of save states)
- Fast-forward (skip frame limiter)
- Run-ahead (two-instance latency reduction)
- Dynamic rate control (proper audio sync)
- Input autoconfig database

---

## Dependency Summary

```toml
[dependencies]
# Core loading
libretro-sys = "0.1.1"
libloading = "0.9"
libc = "0.2"

# Rendering
wgpu = "24"
winit = "0.30"
librashader = { version = "0.10", features = ["wgpu"] }

# Audio
cpal = "0.17"
ringbuf = "0.4"
rubato = "0.16"        # resampling

# Input
sdl2 = "0.37"          # or gilrs for pure Rust

# Overlay (existing)
vello = "0.4"
peniko = "0.3"
parley = "0.4"

# Achievements (via C FFI in build.rs)
# rcheevos vendored, compiled by cc crate

# Networking (for rc_client HTTP + debrid + Yle)
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }

[build-dependencies]
cc = "1"
bindgen = "0.71"
```

---

## Non-Libretro Emulators

Not everything runs as a libretro core. For standalone emulators (Dolphin, RPCS3, PCSX2,
Cemu, etc.), loisto falls back to the Rust configgen subprocess approach — it writes
emulator-specific config files and launches the process. The libretro frontend handles
the retro library; standalone emulators are launched as separate processes under gamescope.

Note: under gamescope, windowing works differently than a traditional desktop. The
frontend runs as an X11 window under gamescope's managed X11 server, so there is no
window manager or desktop environment involved. Gamescope handles display scaling,
resolution, and compositing.

---

## Sources

### Libretro API
- [libretro.h (canonical header)](https://github.com/libretro/RetroArch/blob/master/libretro-common/include/libretro.h)
- [libretro API overview](https://www.libretro.com/index.php/api/)
- [Core development docs](https://docs.libretro.com/development/cores/developing-cores/)
- [OpenGL HW render docs](https://docs.libretro.com/development/cores/opengl-cores/)
- [Dynamic rate control](https://docs.libretro.com/development/cores/dynamic-rate-control/)
- [Frontends list](https://docs.libretro.com/development/frontends/)

### Reference implementations
- [nanoarch](https://github.com/heuripedes/nanoarch) — minimal C frontend (~600 LOC)
- [sdlarch](https://github.com/heuripedes/sdlarch) — SDL2 variant
- [miniretro](https://github.com/davidgfnet/miniretro) — CLI testing frontend
- [go-nanoarch](https://github.com/libretro/go-nanoarch) — Go port, libretro-maintained
- [Ludo](https://github.com/libretro/ludo) — full Go frontend
- [RustroArch tutorial](https://www.retroreversing.com/CreateALibRetroFrontEndInRust)

### Rust crates
- [libretro-sys](https://crates.io/crates/libretro-sys)
- [rust-libretro-sys](https://crates.io/crates/rust-libretro-sys)
- [libloading](https://docs.rs/libloading/latest/libloading/)
- [librashader](https://github.com/SnowflakePowered/librashader) — [crates.io](https://crates.io/crates/librashader)
- [cpal](https://github.com/RustAudio/cpal)
- [rubato](https://crates.io/crates/rubato) — resampling

### rcheevos
- [rcheevos repo](https://github.com/RetroAchievements/rcheevos)
- [rc_client integration wiki](https://github.com/RetroAchievements/rcheevos/wiki/rc_client-integration)
- [RA API docs](https://api-docs.retroachievements.org/)

### Batocera (reference only)
- [Batocera RetroArch wiki](https://wiki.batocera.org/emulators:retroarch)
- [Batocera core paths](https://deepwiki.com/batocera-linux/batocera.linux/4.1-retroarch-and-libretro-cores)
