# Superkonna Theme — Design Document

## Philosophy

Superkonna is an **asset-agnostic** ES theme for Batocera. It ships zero art assets. Users populate assets via fetch scripts on their box. The theme must look correct and performant with any combination of assets — or none at all.

We do not copy implementations from other themes. We learn ideas from their visual design, then implement from scratch using ES-Batocera's native capabilities.

### Inspirations (vibe only, never implementation)

- **Canvas** (Siddy212): reference for modern ES feature usage, native card rendering, `itemTemplate` pattern. NOTE: Canvas targets ES-DE which has features Batocera lacks (see compatibility section).
- **Elementerial** (mluizvitor): inspiration for the flix gamelist concept. Our fork base.
- **Alekfull-NX**: inspiration for the carousel *feel* only — its implementation is the canonical example of what NOT to do (pre-baked card images, no native primitives, stale ES version, everything hardcoded).

### Design Principles

1. **Subtle, situational effects.** Animations and visual effects exist to add polish, not to show off. Each effect is used sparingly, only where it serves a purpose. If an effect draws attention to itself rather than to the content, it's too much. The glint, the crossfade, the scale pop — each appears once, in one context, at low intensity.

2. **Content-first.** The system art, logos, and game covers are the stars. The theme's job is to frame them, not compete with them. Overlays, gradients, and animations serve to enhance legibility and create depth, nothing more.

3. **Graceful degradation.** Every layer is optional. No system art? Card still looks good with the base color. No logo SVG? ES shows the system name as text. No sounds? Silent is fine. The theme never breaks — it just gets simpler.

## Hard Rules

1. **No shipped art assets.** The repo contains only theme code (XML, variables, scripts) and assets with explicit licensing/credits in markdown format. All third-party art (logos, backgrounds, sounds) is fetched on-box via `scripts/fetch-*.sh` at the user's discretion.

2. **Asset-agnostic styling.** The theme must render correctly regardless of what assets the user provides. No assumptions about logo dimensions, aspect ratios, or embedded styling. If something looks bad, the theme is wrong — fix the theme, not the asset.

3. **Native ES primitives only.** Use ES-Batocera's built-in rendering: `<color>` for backgrounds, `<roundCorners>` for rounding, `<storyboard>` for animations, `<itemTemplate>` for card composition, variable interpolation for shared values. Never bake visual effects into SVG files.

4. **Define once, reuse everywhere.** All colors, sizes, corner radii, and spacing are variables. Views reference variables, never hardcode values. If the same value appears twice, it's a variable.

5. **Two views only.** System carousel + flix gamelist. Nothing else to support or maintain.

6. **Performance first.** Every element must justify its existence. No redundant overlays, no invisible elements, no cascading opacity stacks. Fewer elements = faster rendering.

7. **Persist research.** Every technical capability discovered goes into this document. Session context is unreachable after the session ends.

## ES-Batocera vs ES-DE Compatibility

**CRITICAL:** Batocera uses its own EmulationStation fork, NOT ES-DE. Many features in Canvas and other modern themes are ES-DE only.

### Available in Batocera ES

| Feature | Notes |
|---------|-------|
| `<itemTemplate>` | On carousel, gamecarousel, imagegrid, textlist |
| `<roundCorners>` | On image and video elements, pixel values |
| `<storyboard>` | Events: `activate`, `deactivate`, `scroll` |
| `<animation>` | Properties: scale, opacity, color, path, pos, size, rotation, visible, x, y |
| Binding expressions | `{system:theme}`, `{game:name}`, `{global:battery}`, etc. |
| `<colorEnd>` + `<gradientType>` | Gradient fills on elements |
| `<customView>` | Custom views inheriting from base views |
| Multiple `<path>` elements | ES tries each in order (native fallback chain) |
| `${themePath}` | Absolute path to theme root |
| `ifSubset` | Conditional properties/includes (AND via comma, OR via pipe) |
| `{random}` paths | Random game media for system view extras |
| `autoReverse` on animation | Ping-pong animation |
| `repeat="forever"` on storyboard | Infinite looping |
| `linearSmooth` | Bilinear filtering on images |

### NOT Available in Batocera ES

| Feature | Alternative |
|---------|-------------|
| `firstfile("a","b","c")` | Use multiple `<path>` elements (ES tries in order) |
| `exists("path")` / `!exists()` | Let missing images fail gracefully; ES has built-in `logoText` fallback |
| `formatseconds()` | Not available |
| Ternary expressions in `<text>` | Not available |
| `<rectangle>` element | Use 64x64 white PNG tinted via `<color>` |
| `${system.theme}` in `<include>` | Does NOT resolve — per-system includes silently fail |
| `{system:theme}` in `<path>` inside `<itemTemplate>` | Does NOT resolve — use `{system:image}` instead |

### Confirmed Working in itemTemplate (tested 2026-03-23)

| Feature | Notes |
|---------|-------|
| `{system:image}` in `<path>` | Resolves to the system's logo path (defined by `<image name="logo">`) |
| Static `<path>` | `./assets/card-bg.png` etc. — always works |
| `<roundCorners>` | Works on static images; may drop on selected card during scale animation |
| `<storyboard event="activate/deactivate">` | Works — scale, opacity, x position all animate |
| `<color>` tinting | Works on static images |
| `${variable}` references | Static theme variables resolve correctly |

## System View Design

The system view has exactly two visual layers: a full-screen background and the carousel. No text, no metadata, no UI chrome beyond the top bar.

### Background — Crossfading Slideshow

The background fills the entire screen with system art. Multiple images are layered with staggered opacity animations to create a slow crossfade slideshow effect.

```
┌──────────────────────────────────────────┐
│                                          │
│         Full-screen system art           │
│         (crossfading slideshow)          │
│                                          │
│         Dark overlay for contrast        │
│         Bottom vignette for depth        │
│                                          │
│  ┌────┐ ┌────┐ ┌════┐ ┌────┐ ┌────┐    │
│  │card│ │card│ ║CARD║ │card│ │card│    │  ← carousel at bottom
│  │    │ │    │ ║    ║ │    │ │    │    │    (selected card larger)
│  └────┘ └────┘ └════┘ └────┘ └────┘    │
│                                          │
└──────────────────────────────────────────┘
```

Background layers (view-level, outside itemTemplate):
- **System art**: `./assets/systems/${system.theme}.jpg` with crossfade on system change
- **Dark overlay**: reuses `card-bg.png` (1x1 white pixel) tinted with `${bgColor}` at 50% opacity
- **Bottom vignette**: `fade-ver.png` tinted with `${shadowColor}`, anchored at bottom

For the slideshow crossfade, we can layer the current system art with `{system:random:image}` (random game screenshot) behind it, with staggered opacity animations creating a slow blend between them.

### Carousel — Card Composition

The carousel uses `<itemTemplate>` for fully composited cards. The carousel element itself is transparent.

**Carousel geometry** is configurable via variables:
- Width > 1.0 for trailing-off-screen feel (default `1.76`)
- X offset calculated to center: `-(width - 1.0) / 2`
- Height, Y position, item count, logo size — all variables

#### Per-System Card Colors

Each system has a unique color from a curated pastel palette. Colors are assigned to maximize visual distance — no two adjacent cards in the visible wheel should share a color.

The palette is defined in variables and assigned per-system via metadata XML includes. The deploy pipeline can reorder the color assignment to ensure adjacent uniqueness based on the user's system list.

Palette (warm pastels, muted, consistent brightness):

| Index | Color | Name | Used by |
|-------|-------|------|---------|
| 0 | `3D5A80` | Steel blue | ps1, wii |
| 1 | `5B4A6E` | Dusty purple | ps2, gba |
| 2 | `2A6041` | Forest green | xbox, snes |
| 3 | `8B4D3E` | Warm clay | n64, megadrive |
| 4 | `4A6670` | Slate teal | gamecube, psp |
| 5 | `6B5B3E` | Olive brown | dreamcast, nes |
| 6 | `5A3D5E` | Plum | saturn, ds |
| 7 | `3E5B5A` | Dark sage | 3ds, atari |
| 8 | `6E4A4A` | Muted red | arcade, gb |
| 9 | `4A5A3E` | Moss | gbc, pcengine |

Colors are set via per-system metadata includes:
```xml
<!-- _inc/metadata/ps2.xml -->
<theme><variables><cardSystemColor>5B4A6E</cardSystemColor></variables></theme>
```

The `itemTemplate` uses `${cardSystemColor}` for the card base, falling back to `${cardBaseColor}` (a default from the scheme) when no metadata exists.

#### Card Layer Stack

```
┌─────────────────────────────────┐
│  5. SVG Logo (centered)         │  ← {system:theme}.svg, highest z
│  4. Glint sweep (on activate)   │  ← horizontal sweep, subtle
│  3. Gradient overlay             │  ← card-gradient.png, depth
│  2. System art (tinted, faint)   │  ← {system:theme}.webp at 15%
│  1. Opaque base (per-system)     │  ← card-bg.png tinted, roundCorners
└─────────────────────────────────┘
```

**Layer 1 — Base color**: 1x1 white PNG tinted with `${cardSystemColor}`, rounded corners. Always renders.

**Layer 2 — System art texture**: The same system background image used for the full-screen view, but inside the card at very low opacity (15%), tinted to match the card color. Adds subtle texture and per-system identity. Gracefully absent if no art exists.

**Layer 3 — Gradient overlay**: Pre-made top-light-to-bottom-dark gradient PNG. Adds depth and dimension to the flat card color. Low opacity (30%).

**Layer 4 — Glint sweep**: A diagonal highlight gradient image that sweeps horizontally across the card on `activate` event only. Moves from left to right via `x` animation, fades in then out. Subtle — peak opacity 25-30%. Only appears momentarily when a card is newly selected.

**Layer 5 — Logo**: Clean transparent-background SVG, centered in card (`pos 0.5 0.45`, slightly above center to account for gradient weight at bottom). Falls back to PNG, then to ES's built-in `logoText`.

#### Card Animations

- **Activate (selected)**: Scale 1 → 1.08 (200ms EaseOut), glint sweep runs once
- **Deactivate (unselected)**: Scale 1.08 → 1 (250ms EaseOut)
- **Scroll (other cards during navigation)**: No animation (keeps it clean)

**Important**: When storyboards animate `scale` or `opacity`, ES disables its built-in interpolation for those properties. We take full control.

### Utility Primitives (committed to repo)

These are simple generated graphics — no copyright concerns, no licensing issues:

| File | What | Size | Purpose |
|------|------|------|---------|
| `card-bg.png` | Solid white pixel | 1x1 | Tinted via `<color>` for any solid fill (cards, overlays) |
| `card-gradient.png` | White-to-transparent vertical gradient | 1x480 | Card depth overlay |
| `glint.png` | Diagonal highlight band, white-to-transparent | 480x320 | Activation sweep effect |
| `fade-ver.png` | Vertical fade | existing | Bottom vignette |
| `pin.svg` | Small circle | existing | Controller activity indicator |

## Flix Gamelist Design

A `<customView>` inheriting from `gamecarousel`:

```xml
<customView name="flix" inherits="gamecarousel" displayName="Flix">
```

- Full-screen game art background (selected game's fanart/screenshot)
- White/light gradient overlay (console-style, clean feel)
- Horizontal cover art rail using `<gamecarousel>` with `<itemTemplate>`
- Game logo/marquee at top
- Metadata (genre, year, players) as subtle text

Implementation deferred until system carousel is solid.

## Color Architecture

All colors defined in scheme files (`settings/colors/<scheme>/main.xml`):

| Variable | Purpose | Light Value |
|----------|---------|-------------|
| `fgColor` | Primary text | `1E2028` |
| `bgColor` | Background fill | `F2F3F7` |
| `mainColor` | Accent / interactive | `D64060` |
| `onMainColor` | Text on accent | `FFFFFF` |
| `sectColor` | Section dividers | `DFE3EA` |
| `cardColor` | Menu/panel backgrounds | `FFFFFF` |
| `shadowColor` | Shadows, glows, vignettes | `8090A0` |
| `subtleColor` | Secondary/muted text | `6B7280` |
| `cardBaseColor` | Default carousel card base | `1E2840` |

Per-system card colors override `cardBaseColor` via metadata includes.

Opacity suffixes via `${percent.XX}` variables (e.g., `${fgColor}${percent.50}`).

## Variables Architecture

All sizing, timing, and layout values defined once in `variables.xml`:

```xml
<!-- Corner radii (pixel values, overridden per aspect ratio) -->
<cardCornerRadius>20</cardCornerRadius>

<!-- Carousel geometry (configurable) -->
<carouselWidth>1.76</carouselWidth>        <!-- >1 = items trail off-screen -->
<carouselXOffset>-0.38</carouselXOffset>   <!-- -(width-1)/2 to center -->
<carouselY>0.72</carouselY>
<carouselH>0.22</carouselH>

<!-- Card sizing -->
<cardLogoSize>0.18 0.22</cardLogoSize>
<cardLogoScale>1.2</cardLogoScale>
<cardMaxVisible>7</cardMaxVisible>

<!-- Animation timing (ms) -->
<animFast>150</animFast>
<animMedium>250</animMedium>
<animSlow>400</animSlow>

<!-- Layer opacities -->
<artOverlayOpacity>0.15</artOverlayOpacity>
<gradientOverlayOpacity>0.3</gradientOverlayOpacity>
<glintPeakOpacity>0.25</glintPeakOpacity>
<bgOverlayOpacity>0.50</bgOverlayOpacity>

<!-- Font paths -->
<fontDisplay>./assets/fonts/Inter/Inter-Bold.otf</fontDisplay>
<fontBody>./assets/fonts/Inter/Inter-Regular.otf</fontBody>
<fontLight>./assets/fonts/Inter/Inter-Light.otf</fontLight>
```

Aspect-ratio overrides in `settings/aspect/`:
```xml
<!-- 4:3 smaller corner radii -->
<cardCornerRadius>12</cardCornerRadius>
<carouselH>0.25</carouselH>
```

## Storyboard Reference

### Events

| Event | When | Context |
|-------|------|---------|
| `activate` | Item becomes selected | Carousel items, logos |
| `deactivate` | Item loses selection | Carousel items, logos |
| `scroll` | During scroll (other items) | Non-active carousel items |
| (empty) | Default/load | Any element |

### Animatable Properties

| Property | Type | Example |
|----------|------|---------|
| `scale` | float | `1` → `1.08` |
| `opacity` | float | `0` → `1` |
| `color` | hex | `FFFFFF00` → `FFFFFFFF` |
| `pos` | pair | `0.5 0.5` → `0.5 0.48` |
| `size` | pair | `0.9 0.9` → `1 1` |
| `x`, `y` | float | Position components |
| `rotation` | float | `0` → `7.5` |
| `path` | path | Image swap |
| `visible` | bool | Show/hide |

### Easing Modes

`Linear`, `EaseIn`, `EaseInCubic`, `EaseInQuint`, `EaseOut`, `EaseOutCubic`, `EaseOutQuint`, `EaseInOut`, `Bump`

### Animation Attributes

| Attribute | Required | Notes |
|-----------|----------|-------|
| `property` | Yes | What to animate |
| `from` | No | Start value (defaults to current) |
| `to` | No | End value |
| `begin` | No | Delay in ms (default 0) |
| `duration` | No | Duration in ms (default 0 = instant) |
| `mode` | No | Easing (default Linear) |
| `autoReverse` | No | Ping-pong (default false) |
| `repeat` | No | Count or `forever` |
| `enabled` | No | Binding expression for conditional |

### Key Behavior

When any logo/item has a storyboard animating `scale` or `opacity`, the carousel's built-in scale/opacity interpolation is **completely disabled**. You take full control.

## Binding Expressions

### Syntax Distinction

- **`${variable}`** — static, resolved at theme load time. For theme-defined variables.
- **`{type:property}`** — dynamic, resolved per-item in templates. For system/game/global data.

These are two completely different systems. `${system.theme}` (dot, dollar) works in `<include>` paths and `<variables>`. `{system:theme}` (colon, curly braces) works in element content, `<path>`, and binding contexts.

### System Bindings (`{system:...}`)

| Expression | Type | Description |
|------------|------|-------------|
| `{system:name}` | String | Short name (e.g., "snes") |
| `{system:fullName}` | String | Display name |
| `{system:theme}` | String | Theme folder name |
| `{system:manufacturer}` | String | Manufacturer |
| `{system:releaseYear}` | String | Release year |
| `{system:image}` / `{system:logo}` | Path | Logo path from theme |
| `{system:total}` | String | Game count |
| `{system:random}` | Bindable | Random game (chain: `{system:random:image}`) |

### Game Bindings (`{game:...}`)

| Expression | Type | Description |
|------------|------|-------------|
| `{game:name}` | String | Display name |
| `{game:image}` | Path | Screenshot |
| `{game:thumbnail}` | Path | Thumbnail |
| `{game:marquee}` | Path | Marquee/logo |
| `{game:video}` | Path | Video |
| `{game:favorite}` | Bool | Is favorited |
| `{game:genre}` | String | Genre |
| `{game:releaseYear}` | String | Release year |
| `{game:playerCount}` | Int | Player count |

## Asset Fetch Priority (on-box, deploy pipeline)

1. **Saalis** — clean SVG logos from saalis's static assets (highest priority)
2. **Canvas** — clean SVG logos + webp backgrounds (gap-filler)
3. **Alekfull-NX** — sounds + background images only (NO logos)
4. **Elementerial** — logos only if stripped programmatically (baked-in cards removed by script)
5. **ES logoText** — built-in text fallback for anything still missing

Fetch scripts must:
- Never overwrite a higher-priority asset
- Strip baked-in card rects from SVGs programmatically (if the source has them)
- Clean up redundant formats (remove JPG/PNG where SVG exists)
- Run on the box during deploy, never on the dev machine

## File Structure

```
es-theme-superkonna/
  theme.xml              ← entry point, subset selection, includes
  variables.xml          ← shared sizes, fonts, opacity helpers, timing, corner radii
  DESIGN.md              ← this document (source of truth for all design decisions)
  LICENSE.md             ← Elementerial MIT license
  settings/
    colors/
      light/main.xml     ← default color scheme
      dark/main.xml
      scheme.xml         ← wires scheme vars into ES elements
    display/
      view-system.xml    ← system carousel with itemTemplate
      view-flix.xml      ← flix gamelist (customView on gamecarousel)
    fontSize/            ← font size tier overrides
    aspect/              ← aspect ratio overrides (corner radii, carousel height)
  scripts/
    fetch-canvas-assets.sh
    fetch-alekful-assets.sh
  assets/                ← GITIGNORED — populated on-box by fetch scripts
    logos/               ← system SVG logos (transparent bg, user-provided)
    systems/             ← system background images (user-provided)
    sounds/              ← UI sounds (user-provided)
    fonts/               ← Inter family (SIL OFL — committed, clear license)
    card-bg.png          ← 1x1 white pixel utility (committed)
    card-gradient.png    ← vertical gradient utility (committed)
    glint.png            ← diagonal highlight sweep utility (committed)
    fade-ver.png         ← vertical fade utility (committed)
    pin.svg              ← controller dot utility (committed)
```
