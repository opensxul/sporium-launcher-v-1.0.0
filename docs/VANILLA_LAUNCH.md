# Phase 3 — Vanilla, Java and launch core

Implemented against `MASTER_SPEC.txt` v2.5. Completed phases 0–2 remain in place; no database reset is required. The additional settings deserialize with defaults for existing installations.

## User flow

Run `Запустить Sporium.cmd` from the project root. Create a Vanilla instance and choose a version from the official catalog. Releases are visible by default. Settings → Minecraft controls snapshots/pre-releases/RCs, Beta, Alpha and other official types. Filters affect selection lists, not existing instances.

**Install game** downloads and prepares files. **Play offline / Играть офлайн** installs/verifies files and starts the selected version with a local nickname. It does not enable the demo feature or append `--demo`. This applies to modern releases, snapshots and historical Alpha/Beta. The global nickname is saved in SQLite, appears in the header, and is changed in Settings → Accounts. The launch dialog was removed in phase 4; all instances share the globally active profile.

On 2026-09-27 the user cancelled Microsoft integration and clarified that “beta versions” meant demo mode. The unfinished OAuth code and registration instructions were removed. This change does not import MultiMC files or credentials: the existing official downloads already contain the game, and demo mode was a launch argument.

Local identity uses Minecraft's `OfflinePlayer:<nickname>` MD5-based version-3 UUID convention, rather than the former nil UUID. The name must contain 1–16 ASCII letters, digits or underscores. Changing it changes local player identity; a prior demo player's inventory is not automatically migrated. No worlds or saves are deleted. This profile provides local play, LAN and access to servers configured for offline mode, without an authenticated Microsoft session, entitlement claim, Realms or online-mode server access.

Settings → Java detects installed Java and Sporium-managed runtimes. Automatic mode selects the major version required by Mojang metadata and provisions Eclipse Temurin when necessary. An explicit Java path is inspected and must match x64 and the required major version. Automatic RAM uses half the currently available memory, bounded to 1–4 GiB (up to 2 GiB for historical versions).

## Core behavior

- Official version metadata is SHA-1 checked against the manifest. Artifact sizes and SHA-1 hashes, and Java ZIP SHA-256 hashes, are checked before atomic publication.
- Cache hits are verified, partial downloads remain temporary, and failed files are retried up to three times. Phase 8 adds persistent byte-range resume, configurable file concurrency, pause/continuation, speed/ETA and cache controls. See [downloads and repair](DOWNLOADS_REPAIR.md) for boundaries and verification.
- Only approved HTTPS hosts and redirects are allowed by the downloader. Archive extraction rejects traversal, Windows devices/streams, links, overwrites and excessive expansion; managed paths reject Windows reparse points.
- Windows/architecture/version/feature rules build the classpath and arguments. Arguments are separate process parameters, never shell text. Paths with spaces remain intact. Inherited Java injection environment variables are removed.
- Old metadata repeats jinput natives; repeated artifacts are extracted once. `map_to_resources` and virtual assets have separate mappings, and the correct asset tree is passed to the old launchwrapper.
- Native DLLs are re-extracted into fresh staging, then published to the instance. A resolved plan carries Java, classpath, JVM/game arguments, game/assets directories, main class, logging and memory.
- A process-wide download lock and instance file leases prevent conflicting mutations. Windows Job Objects terminate owned children on abnormal launcher exit. Normal window close is blocked while a game or unpaused operation is active; phase 8 permits closing a paused download and explicitly continuing it after restart. Stop requires confirmation because it can lose unsaved game changes.
- Installation errors are typed and localized. Launcher diagnostics record error categories and OS error numbers, not foreign error text or authentication values. Game stdout/stderr is stored in the instance's `logs/sporium-*.log`.

## Local launch verification — Windows x64, 2026-09-27

The rebuilt executable passed 7 game-native checks. Minecraft 26.3 initialized its renderer/audio from the local launch dialog; the owned Java process used `LocalPlay_26` and had no `--demo` argument. The nickname survived a WebView reload and was updated in SQLite after editing it in the launch dialog. Stop, instance locking, cancellation and file verification/retry passed. Unit checks cover modern conditional demo arguments, legacy positional arguments, old release arguments, stable offline UUIDs and upgrading settings without losing preferences. World creation and multiplayer in the new local mode were not exercised.

## Historical phase 3 launch matrix — Windows x64, 2026-09-26

| Official version | Required Java | Mode                  | Observed result                                                                                                              |
| ---------------- | ------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 26.3             | 25            | Demo                  | Game ran over 35 seconds; renderer/audio initialized and `SporiumDemo joined the game` appeared in the integrated world log. |
| 26.4-snapshot-1  | 25            | Demo                  | Game ran over 35 seconds; integrated demo world loaded and rendered.                                                         |
| b1.7.3           | 8             | Local historical test | Game ran over 35 seconds; LWJGL/OpenAL initialized.                                                                          |
| a1.2.6           | 8             | Local historical test | Game ran over 35 seconds; LWJGL/OpenAL initialized and the legacy game loop remained active.                                 |

These are four tested versions, not validation of all 916 catalog entries. Legacy clients still emit their own obsolete resource/skin endpoint and input-device warnings; launcher-provided assets are mapped locally. Multiplayer, controller support and historical worlds were not validated.

## Reproduce

Offline/unit regression checks are the commands in `README.md`. Real game probes are opt-in: they download official game files and may open game windows. Always provide a dedicated absolute test root; never use a normal player-data directory. The probe deliberately stops its own game after 35 seconds and records the log path under `results/`.

```powershell
rtk proxy cargo run --manifest-path src-tauri/Cargo.toml --features dev-tools --bin game-probe -- 'C:/absolute/dedicated-test-root' 26.3
```

Repeat for `26.4-snapshot-1`, `b1.7.3` and `a1.2.6`. This workspace used `.local/game-smoke`.

After all Cargo checks, build the desktop executable, then run:

```powershell
rtk proxy npm.cmd run desktop:build
rtk proxy npm.cmd run test:native
rtk proxy npm.cmd run test:game-native
```

The game-native test requires the `.local/game-smoke` probe fixtures. It exercises real WebView2 IPC, saved filters, four catalog categories, Java discovery/inspection, persisted local nickname, local-client renderer/audio initialization, active-instance protection, confirmed stop, cancellation and cache reuse. It inspects only its owned Java process to assert that `--demo` is absent and the selected nickname is used. Entering a world from the game menu is a player action; the earlier launch logs record historical world checks separately. Artifacts are in `.local/game-native-smoke`; the previous instance/settings suite remains in `.local/native-smoke`. Run probes/tests after builds to avoid Windows executable locks.

## Sources audited during implementation

- [Mojang official version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json), including per-version metadata and its linked official artifacts.
- [Eclipse Adoptium API](https://api.adoptium.net/q/swagger-ui/) and its linked Temurin release packages.
- [reqwest](https://docs.rs/reqwest/latest/reqwest/) and [zip](https://docs.rs/zip/latest/zip/) API documentation.

The user has removed Microsoft authentication from current scope. Phase 4 now provides multiple global local profiles; per-instance defaults are excluded by user instruction. Fabric/Forge/NeoForge are implemented in phases 5–7. See PROFILES_LOADERS.md for current results. MultiMC/Prism imports have not been implemented. Final approved artwork and Windows quick-launch shortcuts remain at phases 14/16.
