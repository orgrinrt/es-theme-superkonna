# API Authentication & Token Management — Research

## TL;DR

Most gaming metadata APIs use static API keys (no refresh needed). The interesting ones are
IGDB (Twitch OAuth2 client credentials, ~58-day tokens, auto-refresh), debrid services
(OAuth2 device code flow — ideal for TV/console), and Trakt.tv (OAuth2 device code, 24h
tokens with rotating refresh tokens). For a console UI, the **device code flow** (show code
on screen, user visits URL on phone) is the gold standard.

---

## Auth Patterns Overview

### Three tiers of complexity

**Tier 1 — Static API key (set once, forget)**
User pastes a key from a website. Stored in config. Sent as query param or header.
No refresh, no expiry (unless revoked).

Services: RetroAchievements, RAWG, GiantBomb, MobyGames, TMDB, TheGamesDB

**Tier 2 — OAuth2 client credentials (server-to-server, auto-refresh)**
App has a client ID + secret. Requests a bearer token. Token expires, app silently
requests a new one. No user interaction after initial setup.

Services: IGDB (via Twitch)

**Tier 3 — OAuth2 device code / PIN flow (user-facing, periodic refresh)**
Show a short code on the TV screen. User visits a URL on their phone. Enters code.
Device receives a token + refresh token. Auto-refresh when token expires.

Services: Real-Debrid, AllDebrid, Premiumize, Trakt.tv

---

## Per-Service Details

### IGDB (Twitch) — Client Credentials

**Flow:** OAuth2 Client Credentials (no user involved — all data is public)

**Registration:** [dev.twitch.tv/console](https://dev.twitch.tv/console) — free Twitch account,
register an app, get Client ID + Client Secret.

**Token request:**
```
POST https://id.twitch.tv/oauth2/token
  ?client_id=YOUR_CLIENT_ID
  &client_secret=YOUR_CLIENT_SECRET
  &grant_type=client_credentials
```

**Response:**
```json
{
  "access_token": "abc123",
  "expires_in": 5011271,
  "token_type": "bearer"
}
```

**Token lifetime:** ~58 days (`expires_in` varies, check dynamically).

**Refresh:** No refresh token. Request a new token when current one expires.
Client credentials flow always issues a fresh token.

**API usage:**
```
GET https://api.igdb.com/v4/games
Headers:
  Client-ID: YOUR_CLIENT_ID
  Authorization: Bearer YOUR_ACCESS_TOKEN
Body (Apicalypse query):
  fields name, cover.url, rating; where name ~ "Mario"*; limit 10;
```

**Rate limits:** 4 requests per second.

**Data access:** All game metadata (games, covers, screenshots, genres, platforms,
companies, release dates, ratings). No user-specific data exists.

---

### ScreenScraper — Credentials Per Request

**Flow:** Developer ID + password + optional user credentials as query parameters.
No OAuth. No tokens. Credentials sent with every request.

**Registration:**
- User account: [screenscraper.fr/membreinscription.php](https://www.screenscraper.fr/membreinscription.php)
- Developer ID: Request via ScreenScraper forum

**Request format:**
```
GET https://www.screenscraper.fr/api2/jeuInfos.php
  ?output=json
  &devid=DEV_ID
  &devpassword=DEV_PASSWORD
  &softname=loisto
  &ssid=USER_LOGIN          # optional user account
  &sspassword=USER_PASSWORD  # optional user password
  &crc=ROM_CRC
  &md5=ROM_MD5
  &sha1=ROM_SHA1
  &systemeid=SYSTEM_ID
  &romnom=ROM_FILENAME
```

**Thread system (concurrent request limits):**

| User tier | Threads |
|-----------|---------|
| Non-registered | 0 (closed during high load) |
| Registered member | 1 |
| Patreon Silver | +1 |
| Patreon Gold | +5 |

Check `ssinfraInfos` endpoint for current limits. Server returns HTTP 423 when
overloaded and closed to non-members.

**Rate limits:** Daily and hourly quotas. HTTP 429 when exceeded.

**Data access:** Game metadata, box art, screenshots, videos — identified by ROM hash
(CRC/MD5/SHA1). Primarily a scraping service for emulation frontends.

---

### Steam Web API — API Key + OpenID

**Three auth mechanisms (pick based on need):**

#### A. API Key (most common)

**Registration:** [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey)
— requires Steam account + domain name.

```
GET https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/
  ?key=YOUR_KEY
  &steamid=76561198XXXXXXXXX
  &include_appinfo=true
```

**Lifetime:** Permanent until revoked.
**Rate limits:** 100,000 calls/day.

**Caveat:** `GetOwnedGames` only works if the target user's profile is **public**.
No way to force access to private profiles with just an API key.

#### B. OpenID 2.0 (user identity only)

For "Login with Steam" — returns SteamID, nothing else. No API access granted.

```
OP Endpoint: https://steamcommunity.com/openid/
Claimed ID: http://steamcommunity.com/openid/id/<steamid>
```

Stateless identity verification. Requires browser redirect — **not suitable for TV UI**.

#### C. Partner OAuth (restricted)

```
Login URL: https://steamcommunity.com/oauth/login
  ?response_type=token
  &client_id=CLIENT_ID
```

Requires Valve partnership. Not available to third-party apps.

**Device code flow:** Not supported by any Steam auth mechanism.

**Practical approach for loisto:** API key for public data. If user wants their library,
they set their Steam profile to public. No device code flow available.

---

### RetroAchievements — Static API Key

**Registration:** Create account at [retroachievements.org](https://retroachievements.org),
get API key from Account Settings → Keys.

**Request format:**
```
GET https://retroachievements.org/API/API_GetGameInfoAndUserProgress.php
  ?z=YOUR_USERNAME
  &y=YOUR_API_KEY
  &g=GAME_ID
  &u=TARGET_USER
```

**Lifetime:** Permanent.
**Device code flow:** None.
**Rate limits:** Undocumented but "fair usage."

**Note:** The `cheevos_token` in `retroarch.cfg` is for the achievement runtime protocol,
NOT the web API. They are different credentials and not interchangeable.

---

### TheGamesDB — Static API Key

**Registration:** Request via [forums.thegamesdb.net](https://forums.thegamesdb.net/viewtopic.php?t=61)
— requires admin approval.

**Two key types:**
- **Public key:** Monthly quota (1,000 queries/month, batched up to 20 items each = 20,000 effective)
- **Private key:** One-time use for full DB mirror downloads

**Lifetime:** Permanent. Quotas reset monthly.
**Device code flow:** None.

---

### GiantBomb — Static API Key

**Registration:** [giantbomb.com/api](https://www.giantbomb.com/api/) — free account, key shown after login.

```
GET https://www.giantbomb.com/api/games/
  ?api_key=YOUR_KEY
  &format=json
  &filter=name:mario
```

**Lifetime:** Permanent.
**Rate limits:** 200 requests per resource per hour, 400 per 15 minutes overall.
**Device code flow:** None.

---

### RAWG — Static API Key

**Registration:** [rawg.io/apidocs](https://rawg.io/apidocs) — free, instant.

```
GET https://api.rawg.io/api/games?key=YOUR_KEY&search=mario
```

**Lifetime:** Permanent.
**Rate limits:** 20,000 requests/month (free tier).
**Device code flow:** None.
**Database size:** 800,000+ games.

---

### MobyGames — Static API Key

**Registration:** Create account at [mobygames.com](https://www.mobygames.com),
get key from profile → API link.

```
GET https://api.mobygames.com/v1/games?title=mario
  -H "api_key: YOUR_KEY"
```

**Lifetime:** Permanent.
**Rate limits:** 1 request per 5 seconds (free tier). Paid tiers: $9.99/mo for faster.
**Device code flow:** None.

---

### Real-Debrid — OAuth2 Device Code Flow

**The best-documented device flow among debrid services.**

**Registration:** Use the open-source public `client_id`: `X245A4XAIBGVM`

#### Step 1 — Get device code

```
GET https://api.real-debrid.com/oauth/v2/device/code
  ?client_id=X245A4XAIBGVM
```

```json
{
  "device_code": "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
  "user_code": "ABCDEF0123456",
  "interval": 5,
  "expires_in": 1800,
  "verification_url": "https://real-debrid.com/device"
}
```

**Display on TV:** "Visit real-debrid.com/device and enter code: **ABCDEF0123456**"

#### Step 2 — Get user-specific credentials (open-source apps)

```
GET https://api.real-debrid.com/oauth/v2/device/credentials
  ?client_id=X245A4XAIBGVM
  &code=DEVICE_CODE
```

```json
{
  "client_id": "USER_SPECIFIC_ID",
  "client_secret": "USER_SPECIFIC_SECRET"
}
```

#### Step 3 — Poll for token (every `interval` seconds)

```
POST https://api.real-debrid.com/oauth/v2/token
  client_id=USER_CLIENT_ID
  &client_secret=USER_CLIENT_SECRET
  &code=DEVICE_CODE
  &grant_type=http://oauth.net/grant_type/device/1.0
```

```json
{
  "access_token": "TOKEN",
  "expires_in": 3600,
  "token_type": "Bearer",
  "refresh_token": "REFRESH_TOKEN"
}
```

#### Token lifetimes

| Token | Lifetime |
|-------|----------|
| Device code | 30 minutes |
| Access token | ~1 hour |
| Refresh token | Permanent (until revoked) |

#### Refresh

```
POST https://api.real-debrid.com/oauth/v2/token
  client_id=ID
  &client_secret=SECRET
  &code=REFRESH_TOKEN
  &grant_type=http://oauth.net/grant_type/device/1.0
```

**Rate limits:** 250 requests/minute.

---

### AllDebrid — PIN Flow

**Custom PIN-based flow (same UX as device code, different protocol).**

**No app registration needed.**

#### Step 1 — Get PIN

```
GET https://api.alldebrid.com/v4.1/pin/get
```

```json
{
  "status": "success",
  "data": {
    "pin": "ABCD",
    "check": "664c3ca2635c99f291d28e11ea18e154750bd21a",
    "expires_in": 600,
    "user_url": "https://alldebrid.com/pin/?pin=ABCD",
    "base_url": "https://alldebrid.com/pin/"
  }
}
```

**Display on TV:** "Visit alldebrid.com/pin and enter: **ABCD**"

#### Step 2 — Poll for API key

```
POST https://api.alldebrid.com/v4/pin/check
  pin=ABCD
  &check=664c3ca2635c99f291d28e11ea18e154750bd21a
```

When activated:
```json
{
  "status": "success",
  "data": {
    "apikey": "abcdefABCDEF12345678",
    "activated": true
  }
}
```

#### Token lifetimes

| Token | Lifetime |
|-------|----------|
| PIN | 10 minutes |
| API key | **Permanent** (until revoked) |

**Rate limits:** 12 req/sec, 600 req/min.

**API usage:** `Authorization: Bearer APIKEY` header on all requests.

---

### Premiumize.me — Device Pairing

Similar pattern to AllDebrid. App requests a pairing code, user enters it on
premiumize.me, app receives an API key.

**API key:** Also available from account settings at premiumize.me/account.
Passed as `apikey` parameter or Bearer header.

**Rate limits:** Not publicly documented.

**Docs:** [premiumize.me/api](https://www.premiumize.me/api)

---

### Trakt.tv — OAuth2 Device Code Flow

**For movie/TV tracking. Relevant for loisto's media playback side.**

**Registration:** [trakt.tv/oauth/applications](https://trakt.tv/oauth/applications) — free, instant.

**Required headers on all API requests:**
```
Content-Type: application/json
trakt-api-version: 2
trakt-api-key: YOUR_CLIENT_ID
```

#### Device code flow

**Step 1 — Get device code:**
```
POST https://api.trakt.tv/oauth/device/code
Content-Type: application/json

{"client_id": "YOUR_CLIENT_ID"}
```

Response: `device_code`, `user_code`, `verification_url`, `expires_in`, `interval`

**Display on TV:** "Visit trakt.tv/activate and enter code: **XXXXXXXX**"

**Step 2 — Poll for token:**
```
POST https://api.trakt.tv/oauth/device/token
Content-Type: application/json

{
  "code": "DEVICE_CODE",
  "client_id": "YOUR_CLIENT_ID",
  "client_secret": "YOUR_CLIENT_SECRET"
}
```

#### Token lifetimes (changed March 2025)

| Token | Lifetime |
|-------|----------|
| Access token | **24 hours** (was 3 months before March 2025) |
| Refresh token | Long-lived, **rotates on each use** |

#### Refresh

```
POST https://api.trakt.tv/oauth/token
{
  "refresh_token": "REFRESH_TOKEN",
  "client_id": "YOUR_CLIENT_ID",
  "client_secret": "YOUR_CLIENT_SECRET",
  "redirect_uri": "urn:ietf:wg:oauth:2.0:oob",
  "grant_type": "refresh_token"
}
```

Returns **new access token AND new refresh token** (rotation — old refresh token
is invalidated).

**Rate limits:** 1,000 GET per 5 minutes, 1 POST/PUT/DELETE per second.

**Without user auth (client_id only):** Public metadata — movies, shows, trending, popular.
**With user auth:** Watch history, ratings, watchlists, collections, calendar, recommendations.

---

### TMDB — Static Bearer Token

**Registration:** [themoviedb.org/settings/api](https://www.themoviedb.org/settings/api) — free.

```
GET https://api.themoviedb.org/3/movie/11
  -H "Authorization: Bearer YOUR_READ_ACCESS_TOKEN"
```

**Lifetime:** Permanent.
**Rate limits:** ~50 req/sec per IP. No daily/monthly cap.
**Device code flow:** None needed. All metadata is public.

**Has v4 user auth** (for ratings/watchlists) but requires browser redirect — not TV-friendly.
Most data doesn't need it.

---

### Yle Areena — No Auth Needed

Public API was deprecated May 2021. Current programmatic access uses the web player's
internal API with hardcoded credentials (documented in `YLE-AREENA.md`).

No user authentication. No developer registration. Just reverse-engineered endpoints.

---

## Device Code Flow Summary

Services with TV/console-friendly auth:

| Service | Flow type | User URL | Code length | Code expiry | Token expiry | Refresh |
|---------|-----------|----------|-------------|-------------|-------------|---------|
| Real-Debrid | OAuth2 device | real-debrid.com/device | 13 chars | 30 min | 1 hour | Yes (permanent refresh token) |
| AllDebrid | PIN | alldebrid.com/pin | 4 chars | 10 min | **Permanent** | N/A (key doesn't expire) |
| Premiumize | Pairing | premiumize.me | varies | varies | Permanent | N/A |
| Trakt.tv | OAuth2 device | trakt.tv/activate | 8 chars | 10 min | 24 hours | Yes (rotating refresh token) |

Services where user pastes a key from their browser:

| Service | Key source URL | Key type |
|---------|---------------|----------|
| RetroAchievements | retroachievements.org → Settings → Keys | Permanent |
| Steam | steamcommunity.com/dev/apikey | Permanent |
| RAWG | rawg.io/apidocs | Permanent |
| GiantBomb | giantbomb.com/api | Permanent |
| MobyGames | mobygames.com → Profile → API | Permanent |
| TheGamesDB | Forum request (admin approval) | Permanent |
| TMDB | themoviedb.org/settings/api | Permanent |

Services where loisto manages its own credentials (no user setup):

| Service | Flow | Notes |
|---------|------|-------|
| IGDB (Twitch) | Client credentials | App's own client_id+secret, auto-refresh ~58 day tokens |
| ScreenScraper | Dev credentials | Dev ID+password, user credentials optional for quotas |
| Yle Areena | None | Hardcoded public credentials |

---

## Recommended Architecture for Loisto

### Token store

```rust
pub struct TokenStore {
    path: PathBuf,  // ~/.config/loisto/tokens.json (encrypted or file-permission-protected)
    tokens: HashMap<String, ServiceToken>,
}

pub enum ServiceToken {
    /// Static API key, never expires
    ApiKey {
        key: String,
    },
    /// OAuth2 client credentials (IGDB)
    ClientCredentials {
        client_id: String,
        client_secret: String,
        access_token: String,
        expires_at: SystemTime,
    },
    /// OAuth2 device code flow (Real-Debrid, Trakt)
    DeviceOAuth {
        client_id: String,
        client_secret: String,
        access_token: String,
        refresh_token: String,
        expires_at: SystemTime,
    },
    /// AllDebrid-style permanent API key from device flow
    DeviceApiKey {
        apikey: String,
    },
    /// ScreenScraper credentials
    ScreenScraper {
        dev_id: String,
        dev_password: String,
        user_login: Option<String>,
        user_password: Option<String>,
    },
}
```

### Auto-refresh middleware

```rust
impl TokenStore {
    /// Get a valid token, refreshing if needed. Returns None if user setup required.
    pub async fn get_valid_token(&mut self, service: &str) -> Option<&str> {
        let token = self.tokens.get_mut(service)?;
        match token {
            ServiceToken::ApiKey { key } => Some(key),
            ServiceToken::DeviceApiKey { apikey } => Some(apikey),
            ServiceToken::ClientCredentials { expires_at, .. } => {
                if SystemTime::now() > *expires_at {
                    self.refresh_client_credentials(service).await.ok()?;
                }
                // return refreshed token
            }
            ServiceToken::DeviceOAuth { expires_at, .. } => {
                if SystemTime::now() > *expires_at {
                    self.refresh_device_oauth(service).await.ok()?;
                }
                // return refreshed token
            }
            _ => { /* ... */ }
        }
    }
}
```

### First-run setup flow (controller-friendly)

```
┌───────────────────────────────────────────────┐
│  ⚙ Account Setup                              │
│                                                │
│  ✓ IGDB              (automatic — no setup)   │
│  ✓ Yle Areena        (automatic — no setup)   │
│                                                │
│  ○ RetroAchievements  [Enter API key]     ▸   │
│  ○ Real-Debrid        [Link device]       ▸   │
│  ○ Trakt.tv           [Link device]       ▸   │
│  ○ Steam              [Enter API key]     ▸   │
│                                                │
│  Skip for now →                                │
│                                                │
│  [A] select  [B] back  [Y] skip all           │
╠═══════════════════════════════════════════════╣
│  Services can be configured later in Settings  │
└───────────────────────────────────────────────┘
```

For device code services:
```
┌───────────────────────────────────────────────┐
│  🔗 Link Real-Debrid                          │
│                                                │
│  On your phone or computer, visit:             │
│                                                │
│       real-debrid.com/device                   │
│                                                │
│  Enter this code:                              │
│                                                │
│          ██  ABCDEF0123456  ██                 │
│                                                │
│  Waiting...  ◐                                 │
│                                                │
│  Code expires in 28:45                         │
│                                                │
╠═══════════════════════════════════════════════╣
│  [B] cancel                                    │
└───────────────────────────────────────────────┘
```

For API key services:
```
┌───────────────────────────────────────────────┐
│  🔑 RetroAchievements                         │
│                                                │
│  Get your API key from:                        │
│  retroachievements.org → Settings → Keys       │
│                                                │
│  API Key: [________________________]           │
│  Username: [_____________________]             │
│                                                │
│  (Use d-pad to select characters,              │
│   or connect a keyboard)                       │
│                                                │
╠═══════════════════════════════════════════════╣
│  [A] confirm  [B] cancel                       │
└───────────────────────────────────────────────┘
```

### Token refresh schedule

| Service | Strategy |
|---------|----------|
| IGDB | Check `expires_at` before each request batch. Refresh proactively at 90% lifetime. |
| Real-Debrid | Access token expires hourly. Refresh proactively 5 min before expiry. Store refresh token persistently. |
| Trakt.tv | 24h tokens. Refresh on first API call of each session. **Save new refresh token immediately** (rotation = old one dies). |
| AllDebrid | API key is permanent. No refresh needed. |
| Static keys | No refresh. Validate on startup (make a test call, warn user if invalid). |

### Security considerations

- Store tokens in `~/.config/loisto/tokens.json` with `0600` permissions
- On Batocera: `/userdata/system/.config/loisto/tokens.json`
- Never log tokens (mask in debug output)
- Refresh tokens are more sensitive than access tokens — losing a refresh token
  means the user must re-authenticate via device flow
- Consider encrypting the token store at rest (optional, since the machine is
  assumed to be single-user)

---

## Rate Limit Summary

| Service | Limit | Notes |
|---------|-------|-------|
| IGDB | 4 req/sec | Per client ID |
| ScreenScraper | 1 thread (free), up to 6 (Patreon Gold) | Plus daily/hourly quotas |
| Steam | 100,000/day | Per API key |
| RetroAchievements | "Fair usage" | Undocumented |
| TheGamesDB | 1,000 queries/month (20 items each) | Monthly reset |
| GiantBomb | 200/hr per resource, 400/15min total | Per API key |
| RAWG | 20,000/month | Free tier |
| MobyGames | 1 req/5sec (free), faster on paid | Per API key |
| Real-Debrid | 250/min | Per user |
| AllDebrid | 12/sec, 600/min | Per user |
| Trakt.tv | 1,000 GET/5min, 1 write/sec | Per user |
| TMDB | ~50/sec | Per IP, no daily cap |

---

## Sources

### IGDB / Twitch
- [IGDB API Docs](https://api-docs.igdb.com/)
- [Twitch Authentication](https://dev.twitch.tv/docs/authentication/)
- [Twitch Developer Console](https://dev.twitch.tv/console)

### ScreenScraper
- [ScreenScraper Registration](https://www.screenscraper.fr/membreinscription.php)
- [Skyscraper scraping modules](https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md)

### Steam
- [Steam Web API Docs](https://steamcommunity.com/dev/)
- [Steam OAuth (Partner)](https://partner.steamgames.com/doc/webapi_overview/oauth)
- [Steam Auth Overview](https://partner.steamgames.com/doc/webapi_overview/auth)

### RetroAchievements
- [RA API Getting Started](https://api-docs.retroachievements.org/getting-started.html)
- [RA API Docs](https://api-docs.retroachievements.org/)

### TheGamesDB
- [TGDB API Key Request](https://forums.thegamesdb.net/viewtopic.php?t=61)
- [TGDB API Docs](https://api.thegamesdb.net/)

### GiantBomb
- [GiantBomb API](https://www.giantbomb.com/api/)

### RAWG
- [RAWG API Docs](https://rawg.io/apidocs)

### MobyGames
- [MobyGames API](https://www.mobygames.com/info/api/)

### Debrid Services
- [Real-Debrid API](https://api.real-debrid.com/)
- [AllDebrid API](https://docs.alldebrid.com/)
- [Premiumize API](https://www.premiumize.me/api)

### Trakt.tv
- [Trakt API Docs](https://trakt.docs.apiary.io/)
- [Trakt Token Changes (March 2025)](https://github.com/trakt/trakt-api/discussions/495)

### TMDB
- [TMDB Getting Started](https://developer.themoviedb.org/docs/getting-started)
- [TMDB Authentication](https://developer.themoviedb.org/docs/authentication-application)
- [TMDB Rate Limiting](https://developer.themoviedb.org/docs/rate-limiting)

### Yle Areena
- [Yle Developer Portal (deprecated)](https://developer.yle.fi/)
