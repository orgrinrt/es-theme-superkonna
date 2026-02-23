# RetroAchievements Integration — Research

## Current State

The overlay already has passive achievement toast notifications:

- `watcher.rs` — tails `/tmp/retroarch.log`, parses `[RCHEEVOS]: awarding cheevo <ID>: <Title> (<Description>)`
- `popup.rs` — toast animation (SlideIn 300ms → Hold 4000ms → FadeOut 500ms → Done)
- `popup.rs` has a `badge_png: Option<Vec<u8>>` field wired but never populated

What's missing: no API communication, no user auth, no badge images, no game
identification, no progress tracking. Strictly a log scraper for toasts.

---

## RetroAchievements Web API

### Authentication

- **Base URL**: `https://retroachievements.org/API/`
- **Auth**: Web API key as query parameter `y` → `?y=<web_api_key>`
- **Getting a key**: User copies from their [RA control panel](https://retroachievements.org/controlpanel.php)
- **User lookup**: parameter `u` accepts username or ULID (stable, usernames can change)
- **Rate limits**: "fair burst" per user, no published numbers

Simplest auth flow for loisto: user provides their Web API key once, stored in config.
No password handling needed.

### Image URLs

```
https://media.retroachievements.org/Badge/<badge_name>.png     # 64x64 achievement badges
https://media.retroachievements.org/Images/<image_id>.png      # game icons/boxart
https://media.retroachievements.org/UserPic/<username>.png     # user avatars
```

API responses return relative paths (e.g., `/Badge/250336.png`).

---

## Key API Endpoints

### User

| Endpoint | Params | Returns |
|----------|--------|---------|
| GetUserProfile | `u` | username, ULID, avatar, points, softcore points, true points, rich presence, last game, motto |
| GetUserRecentAchievements | `u`, `m` (minutes) | array: date, hardcore, ID, title, desc, badge, points, game title/icon/ID, console |
| GetUserCompletionProgress | `u`, `c`, `o` (paginated) | per game: ID, title, icon, console, max/awarded/awarded_hardcore, highest award kind/date |
| GetUserAwards | `u` | total awards, mastery count, beaten count, visible awards array |
| GetUserRecentlyPlayedGames | `u`, `c`, `o` | recently played with playtime |
| GetAchievementsEarnedBetween | `u`, `f`, `t` | achievements in date range |

### Game

| Endpoint | Params | Returns |
|----------|--------|---------|
| GetGameExtended | `i` | full metadata + all achievements with: ID, title, desc, points, true ratio, author, badge, type (standard/missable/progression/win), num awarded |
| GetGameInfoAndUserProgress | `u`, `g` | same as above PLUS per-achievement earned dates, user completion %, total playtime, highest award |
| GetGameLeaderboards | `i`, `c`, `o` | paginated: ID, title, format (VALUE/TIME/SCORE), top entry |
| GetLeaderboardEntries | `i`, `c`, `o` | paginated entries for a specific leaderboard |
| GetGameHashes | `i` | ROM hashes that map to this game |

### System/Feed

| Endpoint | Returns |
|----------|---------|
| GetConsoleIDs | all supported systems with IDs |
| GetAchievementOfTheWeek | current AotW with unlock list |
| GetRecentGameAwards | recent mastery/beaten awards site-wide |
| GetTopTenUsers | top 10 by points |

---

## RetroArch Internals

### How RA talks to the RA server

RetroArch bundles the `rcheevos` C library. Flow:
1. **Login**: `rc_client_begin_login_with_token` (token in `retroarch.cfg` as `cheevos_token`)
2. **Game load**: ROM is hashed, hash resolved to game ID, achievement defs fetched, session starts
3. **Per-frame**: `rc_client_do_frame` reads emulated memory via callback, evaluates conditions
4. **Events**: `RC_CLIENT_EVENT_ACHIEVEMENT_TRIGGERED` fires through event handler
5. **Rich presence**: periodic pings send current state to server

### Log format

```
[INFO] [RCHEEVOS]: awarding cheevo <ID>: <Title> (<Description>)
[INFO] [RCHEEVOS]: login succeeded
[INFO] [RCHEEVOS]: awarded achievement <ID>
```

### retroarch.cfg settings

```ini
cheevos_enable = "true"
cheevos_hardcore_mode_enable = "false"
cheevos_token = "<token>"
cheevos_username = "<username>"
cheevos_badges_enable = "true"
cheevos_test_unofficial = "false"
```

### UDP network commands (port 55355)

`READ_CORE_MEMORY`, `WRITE_CORE_MEMORY`, `PAUSE`, `QUIT`, etc.
**No achievement-specific commands.** Cannot query unlock state via UDP.

---

## rcheevos C Library

### What it provides

- Achievement/leaderboard condition parsing and evaluation (per-frame)
- `rc_client_t` — high-level API for login, game load, session, achievement state
- `rapi` — URL/request builders for all RA server endpoints (does NOT do HTTP)
- `rhash` — ROM hashing for game identification

### Standalone use?

**Partially.** The library does NOT provide HTTP or memory access. You supply callbacks:
- `rc_client_read_memory_func_t` — read emulated memory
- `rc_client_server_call_t` — make HTTP requests

For a **frontend** (not an emulator), two scenarios:

**A. Frontend-only (no running emulator)**: Use `rapi` to build API request URLs, make
HTTP calls yourself, process responses. Gives you: login, game data, achievement lists,
leaderboards, user progress. No per-frame evaluation needed.

**B. Frontend + live emulator**: Use `rc_client_t` with IPC to proxy memory reads from
the running emulator. Heavier, but gives real-time achievement tracking.

### Rust FFI feasibility

rcheevos is plain C, no complex deps. `bindgen` generates bindings from headers.
`cc` crate can compile the C sources into the Rust binary.

**But for loisto, the Web API is more practical.** The frontend doesn't have emulator
memory access. rcheevos FFI only worth it for ROM hashing (`rhash`) or if we later
add emulator memory IPC.

### Existing Rust crates

One exists (`retroachievements-rs`) — abandoned (2022), unpublished, wraps ~6 endpoints,
spawns a runtime per call, all `.unwrap()`. **Not usable.** Build our own.

---

## Integration Architecture for Loisto

### Recommended: Direct Web API client in Rust

```rust
pub struct RaClient {
    username: String,
    api_key: String,
    http: reqwest::Client,
    badge_cache: PathBuf,  // ~/.cache/loisto/ra-badges/
}
```

Dependencies: `reqwest`, `serde`/`serde_json`, `image`, `tokio`.

### Feature-to-Endpoint Mapping

| Loisto Feature | API Endpoint | Notes |
|---------------|-------------|-------|
| User profile in top bar | GetUserProfile | username, points, rank, avatar |
| Per-game achievement list | GetGameInfoAndUserProgress | merges game + user state in one call |
| Recent activity feed | GetUserRecentAchievements | configurable lookback |
| Mastery/completion tracking | GetUserCompletionProgress | paginated, shows award kind per game |
| User badges/awards | GetUserAwards | mastery counts, visible awards |
| Per-game leaderboards | GetGameLeaderboards + GetLeaderboardEntries | two calls |
| Achievement of the Week | GetAchievementOfTheWeek | single call |
| Rich presence display | GetUserProfile | RichPresenceMsg field |
| Hardcore indicator | GetGameInfoAndUserProgress | DateEarnedHardcore vs DateEarned |
| Game badge in library | GetUserCompletionProgress | ImageIcon field |
| Library-wide progress | GetUserCompletionProgress (loop) | paginate through all games |

### Enhancing overlay toasts with badges

The achievement ID is already in the log line. To add badge images:

1. When RetroArch loads a game (detect via `[RCHEEVOS]: login succeeded` in log),
   pre-fetch game's achievement list via `GetGameExtended`
2. Cache all badge images: `~/.cache/loisto/ra-badges/<badge_name>.png`
3. On toast, map achievement ID → badge name from cached list
4. Pass PNG bytes to existing `Popup::with_badge()`

### ROM-to-Game-ID resolution (for library)

Romhoard needs to map ROM files to RA game IDs to show achievement data:

**Option A**: Use rcheevos `rhash` via C FFI to hash ROMs (handles platform-specific
rules), then `GetGameHashes` to resolve.

**Option B**: Bulk fetch via hash library endpoint — one call per console gets all
hash→game_id mappings. Hash local ROMs and look up locally. Better for many ROMs.

Cache the mapping in romhoard's DB.

### Image caching

```
~/.cache/loisto/ra-badges/<badge_name>.png    # 64x64 achievement badges
~/.cache/loisto/ra-icons/<image_id>.png       # game icons
~/.cache/loisto/ra-avatars/<username>.png     # user avatars
```

Fetch on first display, serve from cache. Badges rarely change.

---

## Data Types

```rust
pub struct UserProfile {
    pub user: String,
    pub ulid: String,
    pub user_pic: String,
    pub total_points: u32,
    pub total_softcore_points: u32,
    pub total_true_points: u32,
    pub rich_presence_msg: String,
    pub last_game_id: u32,
    pub motto: String,
}

pub struct GameProgress {
    pub id: u32,
    pub title: String,
    pub console_name: String,
    pub image_icon: String,
    pub achievements: Vec<Achievement>,
    pub user_completion_pct: f32,
    pub user_playtime: u32,
    pub highest_award: Option<AwardKind>,
}

pub struct Achievement {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub points: u32,
    pub true_ratio: u32,
    pub badge_name: String,
    pub author: String,
    pub kind: AchievementType,
    pub num_awarded: u32,
    pub num_awarded_hardcore: u32,
    pub date_earned: Option<String>,
    pub date_earned_hardcore: Option<String>,
}

pub enum AchievementType { Standard, Missable, Progression, Win }
pub enum AwardKind { Mastered, BeatenHardcore, BeatenSoftcore, Completion }

pub struct CompletionEntry {
    pub game_id: u32,
    pub title: String,
    pub image_icon: String,
    pub console_name: String,
    pub max_possible: u32,
    pub num_awarded: u32,
    pub num_awarded_hardcore: u32,
    pub highest_award: Option<AwardKind>,
}

pub struct LeaderboardEntry {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub format: LeaderboardFormat, // Value, Time, Score
    pub entries: Vec<LeaderboardRank>,
}

pub struct LeaderboardRank {
    pub user: String,
    pub rank: u32,
    pub score: i64,
    pub formatted_score: String,
}
```

---

## Sources

- [RA API Docs](https://api-docs.retroachievements.org/)
- [RA API Getting Started](https://api-docs.retroachievements.org/getting-started.html)
- [rcheevos repo](https://github.com/RetroAchievements/rcheevos)
- [rcheevos rc_client wiki](https://github.com/RetroAchievements/rcheevos/wiki/rc_client-integration)
- [RetroArch network commands](https://docs.libretro.com/development/retroarch/network-control-interface/)
- [Badge/icon creation guidelines](https://docs.retroachievements.org/developer-docs/badge-and-icon-creation.html)
