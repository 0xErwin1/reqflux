# ReqFlux design system: Bolt

ReqFlux uses **Bolt**, the design system of DBFlux, with its own accent. ReqFlux and DBFlux are a
family of apps: same structure, typography, shapes and density; one accent per product.

Source: the tokens come from DBFlux's `bolt` crate (`crates/bolt`, `bolt::tokens`,
`bolt::Colors`, `docs/DESIGN_SYSTEM.md`), which ReqFlux takes by git at the DBFlux rev pinned
in `Cargo.toml` (`b33bd9b3c02f1e72b26d77be63d1a73f74260456`). ReqFlux copies none of it: a
Bolt value changes in DBFlux, and ReqFlux follows when it moves the pin. This page restates the
values for the artboards; where it and `bolt` disagree, `bolt` wins.

## What ReqFlux takes from Bolt unchanged

| Area | Values |
|------|--------|
| Neutrals (dark) | desk `#050507`, background `#09090B`, panel `#100F13`, raised `#1A181E`, line `#232128`, line-2 `#37333D`, row divider `#18161B` |
| Text (dark) | strong `#F7F4F7`, body `#C6C3CC`, muted `#8E8996` |
| Washes | hover white 4 %, alternating row white 1.2 %, selected row tint 7 %, selected item tint 12 %, text selection tint 25 %, drop target tint 10 %, scrim desk 62 %, island edge white 5 % |
| Typography | Archivo (interface), Archivo Expanded (uppercase labels, 800, 0.14 em), JetBrains Mono (data, URLs, code, keys); base 13 px; the eight text roles of Bolt (title 20, heading 18, body 13, body small 12, label 11, caption 12, code 13, key hint 12) |
| Shapes | 45 degree cuts on controls and surfaces (keycap 4, control 6, input 8, large control 10, overlay 12, card and island 14, modal 18); data (rows, cells, code) stays square; radius 0 |
| Spacing | 4, 8, 12, 16, 24 px plus a 6 px half step; borders 1 and 2 px; focus ring 1.5 px |
| Chrome | title bar 44, document tab row 46 with 30 px tabs, panel header 40, status bar 38 with 26 px chips, tree row 26 with 14 px indent, controls 30 (24 inline, 44 large) |
| Islands | panel fill cut 14 px at top-left and bottom-right, 1 px island edge along the full outline, no shadow, 8 px of desk between islands and at the side edges |
| Motion | 150 ms, `cubic-bezier(.2, .8, .2, 1)` |

The two Bolt rules about the accent hold for any accent: the accent **fill** is never text on
the dark ground, and accent text, icons, caret and focus ring use the accent **tint**.

## What ReqFlux sets itself

The reference is `design/bolt-v2/`, built from DBFlux's own artboards
(`design/redesign-islands-*` in DBFlux) and screenshots of the running app. When a value or a
component is not listed here, take it from DBFlux, not from a reinterpretation.

| Token | Value | Usage |
|-------|-------|-------|
| `accent` | `#2B45B8` | Primary fills: Send, primary buttons, badges, logo |
| `accent-hover` | `#3451C8` | Primary hover |
| `accent-pressed` | `#1F3389` | Primary pressed |
| `accent-tint` | `#9DB1FF` | Accent text and icons, selection bar, active rail entry, focus ring, caret, resolved variables |
| `on-accent` | `#FFFFFF` | Text on accent fills (7.9:1) |
| Washes | tint at 6, 11, 14, 16 % | Hover, selected item, selected button, active rail entry |

HTTP methods use Bolt's semantic hues, as DBFlux colors its object kinds: GET `#7BE0A0`
(success), POST `#FFC23D` (warning), PUT `#6EA8FF` (info), PATCH `#B79CFF` (null), DELETE
`#FF6B5E` (danger), other verbs `#6FD3D8` (cyan). Status codes use the same semantics: 2xx
success, 3xx info, 4xx warning, 5xx and transport errors danger.

State markers:

- Unsaved draft: a small diamond in `accent-tint`, on the tab, the tree row and the Save action.
- Unresolved variable: `warning` text on a 12 % warning wash with a 1 px underline.
- Resolved variable: `accent-tint` text on a 12 % tint wash.
- Secrets: always masked as `••••••••`, with a `SECRET` badge where a value would show.
- Environments: DBFlux's environment diamonds and tags: Production red, Staging yellow,
  Development blue, Local muted.

## Divergences from Bolt

| Change | Bolt | ReqFlux | Why |
|--------|------|-------|-----|
| Accent | byzantine `#702963`, tint `#D48CC8` | cobalt `#2B45B8`, tint `#9DB1FF` | One accent per product in the family |

The accent is the only divergence. ReqFlux's earlier copy of Bolt differed in the light
palette and in what gpui-component draws; since ReqFlux installs DBFlux's `bolt` alone, those
are DBFlux's values (see "Light theme" below).

ReqFlux has no rules of its own on top of Bolt's composition: earlier "hierarchy rules" drifted
the design away from DBFlux and were dropped.

## Open points

- ReqFlux has no logo yet; the welcome artboard uses a placeholder mark.
- The focus ring does not follow the cut corners in the artboards (CSS `clip-path` removes
  outlines). In GPUI the ring is drawn as its own path, so the app can trace the cuts.
- Light theme: DBFlux's `bolt::Colors::light()` and `PaletteMode::{Dark, Light, System}`:
  desk `#E6E1E9`, background `#F6F4F7`, panel `#FFFFFF`, raised `#EEEAF0`, line `#E3DEE6`,
  line-2 `#CBC4D1`, row divider `#F0ECF2`, strong `#141118`, foreground `#3B3740`, muted
  `#6B6572`, hover `#141118` 4 %, alternating row `#141118` 1.5 %, island edge `#141118` 6 %,
  scrim `#1E1423` 28 %, success `#1C7F45`, info `#1F5FD1`, warning `#B7791F`, danger
  `#C7362B`. ReqFlux's earlier copy had desk `#EDECF0`, background `#F4F3F6`, panel
  `#FCFBFD`, raised `#EFEDF2`, line `#E0DEE5`, line-2 `#C9C5D0`, row divider `#EEECF1`,
  strong `#17151B`, foreground `#3A3740`, muted `#6B6673`, black washes (4, 2, 6 %), scrim
  `#17151B` 32 %, success `#1C7A45`, info `#1F5FCC`, warning `#8A5A00`, danger `#C4352B`.
  In light the washes use the accent (cobalt `#2B45B8`) instead of the tint; a brand cannot
  set its own light tint yet. ReqFlux's light method colors: PATCH `#7445D6`, HEAD/OPTIONS
  `#0E7C83`. The artboards are dark only.
- Dark theme: the roles are unchanged; what gpui-component draws from the theme now follows
  DBFlux: sidebar, list, tab, tab bar and table head `#09090B` (were `#100F13`), title and
  status bar `#09090B` with a `#232128` border (were `#050507`, no border), table head text
  strong, cyan `#6FD3D8`, the code editor's background, active line and line numbers, its
  syntax colors, and window shadows on.
