# Phase 14 — UI and artwork integration

The server page is deferred with phase 13 at the user's request. The user supplied the final master app PNG on 2026-10-05; it is integrated. Phase 14 now waits only for the complete approved built-in instance-logo batch, which the user will supply later. No substitute instance artwork has been invented.

The user also requested more color while preserving a strict layout. `src/styles/branding.css` adds emerald controls/navigation, cream highlights, coral accents and distinct restrained library colors over dark surfaces. Existing layout and reduced-motion behavior remain. Theme editing/low-resource mode are still phase 15.

## Available controls

- Home shows up to three actually recorded recent instances, newest first, with Play. Never-launched instances are excluded. Favorites, loader libraries, shared folders and global nickname profiles remain shared across the launcher.
- Instance cards derive badges from installed metadata, game sessions and game/content job state. Favorites and recent/name/creation ordering are available in the instance library.
- Each instance has nine keyboard-accessible, URL-backed tabs: Overview, Mods, Resource packs, Shaders, Worlds, Datapacks, Settings, History and Logs. Arrow keys and Home/End switch tabs; reloading preserves the tab.
- Content tabs open the existing embedded Modrinth browser in the corresponding category. Minecraft and loader restrictions remain attached to the instance. World import and restore dialogs work from their respective tabs. Short library-read contention is retried before showing a catalogue error.
- Overview shows real enabled/disabled JAR counts and directory size, last launch and existing Java/RAM selection mode. Counts describe files, not dependency-provided/nested mod IDs. Size excludes shared game/Java caches and uses a bounded metadata traversal; a dash means unavailable rather than zero. UUIDs and log paths stay under Details. Logs opens the instance's actual logs directory.
- Settings contains instance appearance, project/Creator Studio controls, loader settings and the future desktop shortcut action. Windows shortcut creation remains phase 16; its button is explicitly disabled with an explanation.
- The active global local profile is visually distinguished. Transfer filenames are under Details while status, progress, speed and controls remain visible. Home/instance layouts were checked at 1400 and 1000 pixels; the existing shell browser suite also checks narrower widths.

## User icons

Settings → Instance icon → Upload image uses a native PNG/JPG/WebP picker. The backend checks bytes instead of trusting extension, bounds input size/dimensions/decoder allocations, decodes and scales to a maximum of 256 × 256 while preserving aspect ratio and alpha, and re-encodes PNG. SVG, truncated/oversized or invalid images cannot replace the previous selection.

Normalized bytes live under `shared/instance-icons/<sha256>.png`; instances store `custom:<sha256>` plus their existing icon-source metadata. Revision checks prevent stale edits. Reads verify cache hashes. Restart and duplication preserve the selection independently of the original source file. Reset changes the selected instance metadata and preserves shared cache bytes. Custom images are sent through bounded IPC data URLs; no arbitrary local-file WebView permission was added.

The approved catalog is deliberately empty until the required batch arrives. Picker/randomizer controls are wired and random choice cannot select an unapproved asset. Automatic assignment remains UUID-stable. Receiving the artwork must update both the approved catalog and automatic assignment using stable `builtin:<id>` references.

## Artwork manifest (app master received; instance logos pending)

| Suggested file                                                  | Purpose                                                                         | Size / format                                                                       | Transparency |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ------------ |
| `sporium-master.png`                                            | Product identity, window/taskbar/EXE icon; installer and shortcuts in phase 16  | Square 1:1 PNG, highest available resolution                                        | Required     |
| `instance-01.png` … `instance-06.png` (up to `instance-08.png`) | Built-in picker, automatic/random selection, instance cards and later shortcuts | Complete set of 6–8 square PNGs, 1024 × 1024 recommended, consistent style, no text | Required     |

App master: `src-tauri/icons/master/sporium-master.png`, byte-identical to the supplied file (SHA256 `800b0ac97e26c6ff5a7c4480cc15ad43cbae48de422c367228cf775d2690712c`). Run `npm run brand:icons` to produce transparent high-quality scaled PNG/ICO frames at 16/24/32/48/64/128/256 in `src-tauri/icons/generated` and UI PNGs under `public/branding`. The script does not crop, recolor, redraw or replace transparency. `BrandMark` uses these on startup, sidebar and About; the favicon uses the 32px variant. `src-tauri/tauri.conf.json` configures the generated app/Windows icon. Native Windows extraction from the built EXE confirms the supplied mushroom mark. Installer resources inherit this configured artwork when phase 16 implements packaging; no installer was built here.

The instance catalog lives in `src-tauri/src/instances/icons.rs`; `InstanceAppearance` renders its supplied stable IDs and images. Integration of that catalog, automatic assignment and small-size checks remain pending the separate 6–8 approved logo files. The app master is not silently used as an instance-logo substitute.

## Verification

Rust tests cover PNG/JPEG/WebP decoding, scaling/aspect, corrupt and oversized inputs, revision conflict, restart, duplicate, reset, cache tampering and real directory metrics. `scripts/ui-polish-native-smoke.mjs` covers the embedded EXE with real IPC, recent ordering, sorting/favorites, cached custom image rendering, nine tabs/keyboard/reload, contextual Modrinth dialog, world/restore dialogs, icon reset, artwork-gate state, global profile and restart. Native tests use isolated `.local` directories. Historical launch timestamps and a tiny cached image are explicit test fixtures; they are not a claim of a new Minecraft launch or final artwork.

The OS file picker's fields are not automated; decoding/import/persistence is exercised directly in Rust and the native UI checks the upload entry point. Previous real launch/download coverage remains recorded separately. Current counts and the final build/backup are recorded in `PROJECT_STATE.md`.
