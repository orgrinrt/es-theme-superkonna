# Media Playback & Streaming — Research

## TL;DR

mpv is the right backend — it ships on Batocera, handles every codec/HDR/subtitle/audio
passthrough scenario, and has good Rust bindings. Control via IPC socket (`mpvipc` crate)
for v1, upgrade to embedded libmpv (`libmpv2` crate) for seamless UI integration later.
Debrid services all return direct HTTPS URLs that mpv plays natively.

---

## Rust Video Playback Crates

### mpv bindings (recommended)

| Crate | Approach | Notes |
|-------|----------|-------|
| `libmpv2` | Links libmpv C API directly | Requires mpv >= 0.35.0. Best for embedding. |
| `mpvipc` | JSON IPC over Unix socket | Spawns mpv process, controls via commands. Simplest. |
| `mpv-socket` | Alternative IPC crate | Similar to mpvipc |
| `mpv-rs` | Older safe bindings | Has SDL2 embedding example |

**Why mpv?** It handles 100% of the hard problems:

| Feature | mpv support |
|---------|-------------|
| Codecs (H.264/5, VP9, AV1, MPEG-2/4) | Yes (via FFmpeg) |
| Subtitles (ASS, SRT, PGS, WebVTT, CEA-608) | Yes (libass) |
| Audio passthrough (TrueHD, DTS-HD, Atmos) | `--audio-spdif=ac3,eac3,dts-hd,truehd` |
| HDR passthrough | `--target-colorspace-hint=yes` |
| Dolby Vision (Profile 5/8) | Yes (libplacebo, mpv 0.37+) |
| HDR tone mapping (for SDR displays) | `--tone-mapping=auto` |
| Hardware acceleration | `--hwdec=auto` (VAAPI/NVDEC/VDPAU) |
| HLS/DASH adaptive streaming | Yes (FFmpeg demuxer) |
| HTTP progressive download with seeking | Yes |
| Chapter support | Yes |
| Deinterlacing | Yes |

Batocera ships mpv 0.40.0. Zero build dependencies for playback.

### GStreamer (`gstreamer-rs`)

Full pipeline framework. Actively maintained Rust bindings by GStreamer core team.
Batocera ships GStreamer 1.24.8.

**Pros:** Fine-grained pipeline control, VA-API elements, HLS/DASH demuxers.
**Cons:** Verbose pipeline construction, poor subtitle rendering (no libass), limited
HDR/Dolby Vision support, audio passthrough requires manual pipeline work.
~2000 LOC minimum vs ~500 for mpv wrapper.

### FFmpeg (`ffmpeg-the-third`)

Actively maintained fork of `ffmpeg-next`. Wraps FFmpeg C API.
Good for demuxing/decoding, but you'd be reimplementing a video player from scratch:
subtitle rendering, A/V sync, seeking, tone mapping, audio passthrough, frame timing.
~10,000+ LOC, 6-12 months for production quality.

### symphonia (pure Rust audio)

Pure Rust audio decoder (MP3, AAC, FLAC, Vorbis, WAV, ALAC). No video.
Useful for audio-only streams (podcasts, Yle radio) if you want zero C deps for that path.

---

## Hardware Acceleration on Batocera

Batocera x86_64 ships the full stack:

| Component | Version | Notes |
|-----------|---------|-------|
| Mesa + VA-API drivers | Current | Intel iHD/i965, AMD radeonsi |
| libva | 2.22.0 | VA-API runtime |
| FFmpeg | 7.1 | VA-API + CUDA support |
| mpv | 0.40.0 | VA-API hardware decode |
| GStreamer | 1.24.8 | VA-API plugin |
| NVIDIA CUDA | Bundled | Proprietary driver path |

**You don't need to ship any video drivers.** Just use `--hwdec=auto` and mpv
auto-detects the best decoder (VA-API for Intel/AMD, NVDEC for NVIDIA).

---

## Rendering Decoded Video

### Option A: mpv renders to its own window (simplest)

mpv opens a fullscreen X11 window. Gamescope composites it as the focused app.
Control via IPC socket. When playback ends, window closes, focus returns to launcher.

### Option B: mpv renders into your window (libmpv render API)

Use `mpv_render_context_create()` with OpenGL FBO target. mpv renders into your
existing graphics context. Gives seamless UI transitions.

**Note:** Vulkan render API is in development but not yet stable. OpenGL works now.

### Option C: Decode with FFmpeg, render with wgpu

Upload NV12 frames as wgpu textures, write YUV→RGB shader. Maximum control but
essentially building a video player from scratch. Not recommended.

### Gamescope integration

Video player should be a **regular X11 window** that gamescope composites as
the focused app — NOT an overlay. `GAMESCOPE_EXTERNAL_OVERLAY` atom is claimed
by MangoHud on Steam Deck / gamescope-session.

Alternatively, use libmpv render API to render video into the overlay process's
own wgpu/OpenGL context for seamless transitions.

---

## Debrid Service Integration

All three services follow the same pattern: submit a link or magnet, get back a
direct HTTPS download URL. mpv plays these natively with seeking support.

### Real-Debrid

**Base:** `https://api.real-debrid.com/rest/1.0/`
**Auth:** Bearer token (`Authorization: Bearer {token}`) or `?auth_token={token}`
**Rate limit:** 250 req/min

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/unrestrict/link` | POST | Hoster URL → direct download link |
| `/torrents/addMagnet` | POST | Submit magnet link |
| `/torrents/selectFiles` | POST | Choose files from torrent |
| `/torrents/info/{id}` | GET | Status + download links |
| `/streaming/transcode/{id}` | GET | HLS/DASH/MP4 stream URLs |

**Transcode formats:** Apple M3U8 (HLS), DASH MPD, Live MP4, H264 WebM.

### AllDebrid

**Base:** `https://api.alldebrid.com/v4.1/`
**Auth:** `apikey` + `agent` query params
**Auth flow:** PIN-based (`/pin/get` → user enters pin → poll `/pin/check`)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/link/unlock` | GET | Unrestrict hoster link |
| `/magnet/upload` | POST | Submit magnet |
| `/magnet/status` | GET | Check status |
| `/magnet/instant` | GET | Check if cached (instant play) |

### Premiumize.me

**Base:** `https://www.premiumize.me/api/`
**Auth:** API key as query param

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/transfer/directdl` | POST | Direct download (link or magnet) |
| `/cache/check` | GET | Check if cached |
| `/transfer/create` | POST | Async transfer |

### Debrid playback flow

```
User selects content
    ↓
Check cache: /cache/check or /magnet/instant
    ↓ (cached = instant)
Unrestrict: /unrestrict/link or /transfer/directdl
    ↓
Direct HTTPS URL to .mp4/.mkv
    ↓
mpv.loadfile(url)
```

For uncached torrents: submit magnet → poll status → once ready → unrestrict → play.

---

## Architecture Options Comparison

| Criterion | mpv wrapper | GStreamer | FFmpeg+wgpu | VLC |
|-----------|-------------|-----------|-------------|-----|
| Build complexity | Very Low | Medium | High | Medium-High |
| Subtitle support | Excellent | Poor | None (DIY) | Good |
| HDR/Dolby Vision | Excellent | Limited | None (DIY) | Good |
| Audio passthrough | Excellent | Manual | None (DIY) | Good |
| HLS/DASH | Yes | Yes | Manual | Yes |
| Batocera ships it | Yes (0.40.0) | Yes (1.24.8) | Yes (7.1) | No |
| Gamescope integration | Easy | Medium | Hard | Medium |
| Time to working player | Days | Weeks | Months | Weeks |
| Rust LOC needed | ~500 | ~2000 | ~10000+ | ~1000 |

---

## Recommended Architecture

```
┌─────────────────────────────────────────────────┐
│                   loisto-player                   │
│                  (Rust binary)                    │
│                                                   │
│  ┌──────────────┐  ┌─────────────────────────┐   │
│  │ Debrid Client │  │   Yle Areena Client     │   │
│  │               │  │                         │   │
│  │ - Real-Debrid │  │ - player.api.yle.fi     │   │
│  │ - AllDebrid   │  │ - HLS manifest extract  │   │
│  │ - Premiumize  │  │ - app_key + geo headers │   │
│  └──────┬───────┘  └────────┬────────────────┘   │
│         │                    │                     │
│         └────────┬───────────┘                     │
│                  │                                  │
│           resolved URL                              │
│           (HTTPS .mp4/.mkv or .m3u8)               │
│                  │                                  │
│         ┌────────▼──────────┐                      │
│         │   mpv Controller  │                      │
│         │                   │                      │
│         │  mpvipc / libmpv2 │                      │
│         │  --hwdec=auto     │                      │
│         │  --audio-spdif=.. │                      │
│         │  --sub-auto=fuzzy │                      │
│         └────────┬──────────┘                      │
│                  │                                  │
└──────────────────┼──────────────────────────────────┘
                   │
            ┌──────▼──────┐
            │  mpv process │  (or libmpv in-process)
            │  fullscreen  │
            │  X11 window  │
            └──────────────┘
                   │
            ┌──────▼──────┐
            │  gamescope   │  (composites as focused app)
            └─────────────┘
```

### Rust dependencies

```toml
[dependencies]
# mpv control (pick one)
mpvipc = "2"          # IPC socket control (v1)
# libmpv2 = "5"       # Direct embedding (v2)

# HTTP + JSON for API calls
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
url = "2"

# HLS manifest parsing (for Yle)
m3u8-rs = "6"
```

### Implementation phases

1. **Phase 1: mpv IPC player** — spawn mpv, control via socket, play URLs
2. **Phase 2: Debrid client** — Real-Debrid API, magnet→URL resolution, cache check
3. **Phase 3: Yle Areena client** — preview API, HLS manifest extraction
4. **Phase 4: UI integration** — upgrade to libmpv embed for seamless transitions
5. **Phase 5: Additional debrids** — AllDebrid, Premiumize support

---

## Sources

- [Real-Debrid API](https://api.real-debrid.com/)
- [AllDebrid API](https://docs.alldebrid.com/)
- [Premiumize API (SwaggerHub)](https://app.swaggerhub.com/apis-docs/premiumize.me/api)
- [libmpv2 (docs.rs)](https://docs.rs/crate/libmpv2/latest)
- [mpvipc (docs.rs)](https://docs.rs/mpvipc/latest/mpvipc/struct.Mpv.html)
- [mpv render API examples](https://github.com/mpv-player/mpv-examples/tree/master/libmpv)
- [mpv IPC protocol](https://github.com/mpv-player/mpv/blob/master/DOCS/man/ipc.rst)
- [ffmpeg-the-third (GitHub)](https://github.com/shssoichiro/ffmpeg-the-third)
- [gstreamer-rs (GitHub)](https://github.com/sdroege/gstreamer-rs)
- [GStreamer VA-API](https://github.com/GStreamer/gstreamer-vaapi)
- [symphonia (crates.io)](https://crates.io/crates/symphonia)
- [Batocera mpv package](https://github.com/batocera-linux/batocera-x86_64_efi/)
- [Hardware video acceleration (ArchWiki)](https://wiki.archlinux.org/title/Hardware_video_acceleration)
- [Gamescope architecture](https://deepwiki.com/ValveSoftware/gamescope/2-architecture)
