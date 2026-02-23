# Yle Areena Integration — Research

## TL;DR

Yle Areena serves unencrypted HLS streams with static public API credentials.
Resolve stream URL via `player.api.yle.fi`, feed to mpv/GStreamer. No DRM for
most content. Subtitles are segmented WebVTT (needs merging). Finland-only geo.

---

## How Areena Streams Work

### Stream resolution flow

1. Parse program ID from URL: `areena.yle.fi/1-XXXXXXX` → `1-XXXXXXX`
2. Call preview API with static credentials
3. Response contains HLS manifest URL (`.m3u8`)
4. Feed manifest to any HLS-capable player

### Preview API

```
GET https://player.api.yle.fi/v1/preview/{program_id}.json
    ?language=fin
    &ssl=true
    &countryCode=FI
    &host=areenaylefi
    &app_id=player_static_prod
    &app_key=8930d72170e48303cf5f3867780d549b
    &isPortabilityRegion=true

Headers:
    Origin: https://areena.yle.fi
    Referer: https://areena.yle.fi/
```

Credentials are **static and public** — embedded in Areena's web player JavaScript.
No user registration, no OAuth, no per-user tokens.

### Media ID prefixes (from response)

| Prefix | Hosting | Notes |
|--------|---------|-------|
| `29-` | Standard HTML5 | Most common, current Areena content |
| `55-` | Full-HD | Higher quality variant |
| `67-` | Akamai alternative | `yleawsmpodamdipv4.akamaized.net` |
| `78-` | Podcasts | Direct MP3/MP4 download URL |
| `84-`, `85-` | Newer hosting | Recent migration targets |
| `10-` | Live media | Live TV/radio |
| `57-` | Kaltura (legacy) | Deprecated since Dec 2023 |

### Publication status

Response includes `publicationEvent` with temporal status:
- `pending` — not yet available (future publish timestamp)
- `currently` — active content window
- `expired` — no longer accessible

---

## Stream Formats

| Format | Usage | Notes |
|--------|-------|-------|
| **HLS** (`.m3u8`) | Primary, all content | Multi-bitrate adaptive, Akamai CDN |
| **DASH** (`.mpd`) | Legacy (Kaltura era) | Being phased out |
| **Progressive HTTP** | Podcasts only | Direct download |

Quality levels available via HLS adaptive bitrate. Full-HD for `55-` prefixed content.

---

## Subtitles

Areena delivers subtitles as **segmented WebVTT** — 400+ individual `.vtt` segment
files referenced from the HLS manifest, each containing 1-2 lines of dialogue.

**Challenges:**
- Segments frequently overlap → need deduplication after merging
- Available languages: Finnish (`fi`), Swedish (`sv`), sometimes others
- Audio description tracks available for some content

**Handling options:**
- Let mpv handle it natively (best option — handles segmented WebVTT)
- Use yle-dl's `--sublang fi` to download and merge
- Use [pekman/yle-subtitle-dl](https://github.com/pekman/yle-subtitle-dl) for live streams

---

## DRM

**Most Yle-produced content is NOT DRM-protected.** Streams are standard unencrypted HLS.

Exceptions:
- Licensed third-party content (foreign films, some sports) may have DRM
- DRM content simply won't play — no Widevine/FairPlay handling needed
- News content has some access restrictions

---

## Geographic Restrictions

- Most content is **Finland-only** (Finnish IP required)
- EU portability rules: Finnish residents can access within EU
- Small number of videos available internationally
- VPN/proxy to Finnish endpoint works

---

## Official API Status

The old Yle developer API (`external.api.yle.fi`) was **deprecated in May 2021**.
Endpoints are non-functional. The developer portal still has docs online but they're
dead. The Kodi plugin that used it is abandoned.

The **player preview API** is what works now — it's the internal API the web player
uses. yle-dl and yt-dlp both use it. It's undocumented and changes ~2-3 times/year.

---

## Integration Approaches

### Option A: Direct API calls from Rust (recommended for loisto)

```rust
async fn resolve_areena_stream(program_id: &str) -> Result<String> {
    let resp = reqwest::Client::new()
        .get(format!(
            "https://player.api.yle.fi/v1/preview/{program_id}.json"
        ))
        .query(&[
            ("language", "fin"),
            ("ssl", "true"),
            ("countryCode", "FI"),
            ("host", "areenaylefi"),
            ("app_id", "player_static_prod"),
            ("app_key", "8930d72170e48303cf5f3867780d549b"),
            ("isPortabilityRegion", "true"),
        ])
        .header("Origin", "https://areena.yle.fi")
        .header("Referer", "https://areena.yle.fi/")
        .send().await?
        .json::<serde_json::Value>().await?;

    // Extract manifest URL from response
    // Path varies: data.ongoing_ondemand.manifest_url or similar
    let manifest = resp["data"]["ongoing_ondemand"]["manifest_url"]
        .as_str()
        .ok_or("no manifest URL")?;

    Ok(manifest.to_string())
}
```

Feed the resolved m3u8 URL to mpv via libmpv or to GStreamer's `hlsdemux`.

**Fragility**: The JSON response structure changes occasionally. yle-dl has adapted
~2-3 times per year. Using yt-dlp as fallback provides a buffer.

### Option B: yt-dlp as subprocess (most resilient)

```rust
let output = Command::new("yt-dlp")
    .args(["--dump-json", &format!("https://areena.yle.fi/{program_id}")])
    .output()?;
let meta: Value = serde_json::from_slice(&output.stdout)?;
// meta contains: formats[], subtitles{}, title, description, etc.
```

yt-dlp's `--dump-json` gives complete metadata in well-structured JSON. Its Areena
extractor is actively maintained. Can also use `--get-url` for just the stream URL.

### Option C: yle-dl --showurl

```rust
let output = Command::new("yle-dl")
    .args(["--showurl", &format!("https://areena.yle.fi/{program_id}")])
    .output()?;
let stream_url = String::from_utf8(output.stdout)?.trim().to_string();
```

Lightest subprocess approach — prints resolved URL without downloading.

### Option D: mpv-yledl pattern

The [mpv-yledl](https://github.com/pekkarr/mpv-yledl) plugin shows the simplest
integration: resolve URL → pass to mpv. mpv handles HLS, subtitles, audio tracks.

---

## Content Discovery

The old catalog API is dead. Options for browsing/searching Areena content:

1. **Web scraping** `areena.yle.fi` — fragile but comprehensive
2. **yt-dlp** can extract playlists and search results
3. **Build a catalog** by scraping program pages and caching metadata in romhoard's DB
4. **RSS feeds** — Yle publishes RSS for some content categories

For loisto, the practical approach is probably a curated set of live TV channels
(known static URLs) plus on-demand content accessed by URL/search.

### Live TV channels (static HLS URLs)

```
Yle TV1:  http://yletv.akamaized.net/hls/live/622365/yletv1fin/index.m3u8
Yle TV2:  http://yletv.akamaized.net/hls/live/622366/yletv2fin/index.m3u8
Yle Fem:  http://yletv.akamaized.net/hls/live/622367/yletvfemfin/index.m3u8
```

These are publicly known and used by IPTV aggregators.

---

## Legal

- Yle is publicly funded (Yle tax on Finnish residents)
- No subscription for most content
- The preview API credentials are public (embedded in web player JS)
- yle-dl has operated openly since ~2010, no legal action from Yle
- The old developer API explicitly encouraged third-party development
- Content licensing restrictions apply to third-party content
- Finland has no DMCA equivalent; EU Copyright Directive applies
- **Practical risk**: for personal, non-commercial use of Yle's own productions
  within Finland, programmatic access is widely tolerated

---

## Recommended Architecture for Loisto

```
User selects Areena content (browse/search/live TV)
    ↓
loisto resolves stream URL:
    - Live TV: use static m3u8 URLs
    - On-demand: call preview API directly (Option A)
    - Fallback: yt-dlp --dump-json (Option B)
    ↓
Feed m3u8 URL to media player:
    - mpv via libmpv (handles HLS, subtitles, audio tracks natively)
    - Or GStreamer hlsdemux pipeline
    ↓
Overlay sends: bar:context media
    - Bars stay visible (or auto-hide after 5s, reappear on input)
    - Bottom bar shows: [A] play/pause [B] back [LB/RB] seek
```

**Key insight**: mpv already handles everything — HLS adaptive streaming, segmented
WebVTT subtitles, multi-audio tracks, hardware decoding. The "hard part" is just
resolving the stream URL, which is a single API call.

---

## Sources

- [aajanki/yle-dl](https://github.com/aajanki/yle-dl) (latest: 20250126)
- [yt-dlp Yle Areena extractor](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/yle_areena.py)
- [mpv-yledl plugin](https://github.com/pekkarr/mpv-yledl)
- [finnhubb/plugin.video.areena (Kodi)](https://github.com/finnhubb/plugin.video.areena)
- [Yle Developer Docs](https://developer.yle.fi/en/index.html) (deprecated)
- [Yle subtitle extraction blog post](https://killergerbah.github.io/engineering/2025/01/01/extracting-subtitle-data-from-yle-areena.html)
- [pekman/yle-subtitle-dl](https://github.com/pekman/yle-subtitle-dl)
