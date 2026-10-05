# Phases 4–7 — global local profiles, skins and mod loaders

Scope follows the user's 2026-09-27 corrections: Microsoft sign-in is removed; profiles are global and all profiles share exactly the same instance library. Existing phases 0–3 are retained.

## Try it

Run `Запустить Sporium.cmd`. In Settings → Accounts, rename the initial local profile or create/select another one. The active nickname appears in the header and on every instance. Play offline starts directly with this nickname. There are no per-instance profile defaults or account selection dialogs. Switching profiles does not move, duplicate, delete or change instances, mods or world files. Minecraft derives its offline UUID from the nickname, so changing the nickname can change the player inventory associated with a world; Sporium does not migrate player data.

Create a Fabric, Forge or NeoForge instance from the shared library. Its page offers compatible loader versions. Automatic mode chooses a stable version when one exists and pins the version after installation. Install/repair and Play offline use the same installation pipeline. Incompatible combinations report an unsupported version instead of launching Vanilla under a loader label.

## Skins

The launcher resolves a public Mojang skin by nickname without credentials and shows its head and body in the profile UI. Found skins are cached for 24 hours; missing skins for 15 minutes. Refresh explicitly checks again; a service outage can use the previous cached skin and does not disable local play.

The global “Скины по нику в игре” setting installs the unmodified **CustomSkinLoader 15.0.1 Universal** mod for compatible Fabric/Forge/NeoForge versions listed in `src-tauri/skinmod.json`. The binary is fetched from the author's Modrinth project and verified against its pinned SHA-1 and size. The initial configuration uses only Mojang and enables its local profile cache. Existing custom configurations and user-installed CustomSkinLoader files are preserved. Turning the setting off removes only the exact byte-for-byte Sporium-managed JAR on the next preparation of each instance. Changing it does not alter a running game. A network failure during optional skin-module installation does not block the game.

Unmodified Vanilla, unsupported Minecraft versions, and clients without a compatible skin module retain launcher preview only. The current pinned module does not list Minecraft 26.3. Skin appearance is client-side: it does not guarantee what other LAN/server players see. Neither the skin lookup nor the mod grants authentication or replaces the local offline UUID.

**Доступны одиночная игра и LAN. Realms и серверы с обязательной проверкой Microsoft-аккаунта требуют авторизации.**

## Installation and validation

Fabric uses official Fabric Meta profiles, merged arguments and Maven libraries. Forge and NeoForge use SHA-1-verified official installers in private shared staging roots. Java runs without a shell; owned process groups support cancellation. Runtime artifacts and processor outputs are checked before a hash receipt is published; cached output is rechecked on reuse. Shared libraries retain normal Maven paths for module-path arguments. BootstrapLauncher ignores the content-addressed original client JAR to avoid a duplicate Minecraft module. User mods and worlds are never installer staging directories.

Real Windows x64 probes passed with renderer/audio initialization and at least 35 seconds alive:

| Minecraft | Loader             | Java | Fixture result                                     |
| --------- | ------------------ | ---- | -------------------------------------------------- |
| 1.21.1    | Fabric 0.19.5      | 21   | `.local/modded-smoke/results/fabric-1.21.1.json`   |
| 1.12.2    | Forge 14.23.5.2864 | 8    | `.local/modded-smoke/results/forge-1.12.2.json`    |
| 1.20.1    | Forge 47.4.23      | 17   | `.local/modded-smoke/results/forge-1.20.1.json`    |
| 1.21.1    | NeoForge 21.1.252  | 21   | `.local/modded-smoke/results/neoforge-1.21.1.json` |

These probes include the skin module; startup logs show its loader hooks. They verify startup, not a full modpack, an in-world skin render, or LAN/online multiplayer. The first NeoForge attempt exited cleanly before the probe's time threshold; the repeated run completed. Pre-1.12 Forge installer formats have not been validated. General mod browsing, import and content management remain later phases.

Profile regression tests cover migration from existing saved nicknames, revision conflicts, global selection, rename/delete, preservation of the last profile and unchanged shared library/world bytes. Loader tests cover Maven path traversal, inherited library replacement, matching NeoForge's exact Minecraft branch and the original-client ignore argument.

Final checks: 44 Rust tests, 3 frontend tests, 4 browser tests, 15 native settings/library checks, 7 game-native checks and 5 modded-native checks passed. The native skin check uses an actual Mojang texture, verifies nonempty canvas pixels and a visible legacy skin face, and checks persistence of the global skin setting. The loader pages show the pinned versions and same global nickname for all four probe instances. Results and reviewed screenshots are under `.local/native-smoke`, `.local/game-native-smoke` and `.local/modded-native-smoke`. ESLint, Prettier, Clippy with denied warnings, generated bindings and the standalone Tauri build passed.

Reproduce a probe with a dedicated test root only:

```powershell
rtk proxy cargo run --manifest-path src-tauri/Cargo.toml --features dev-tools --bin game-probe -- 'C:/absolute/test-root' 1.21.1 neoforge 21.1.252 Notch
```

The optional final nickname changes only that test root's global profile. Build the desktop executable after probes end, then run `test:native` and `test:game-native`. Windows locks running executables.

## Sources and attribution

- [Fabric Meta](https://github.com/FabricMC/fabric-meta) and [Fabric Maven](https://maven.fabricmc.net/).
- [Forge installer](https://github.com/MinecraftForge/Installer) and [official Maven](https://maven.minecraftforge.net/).
- [NeoForge client documentation](https://docs.neoforged.net/user/docs/client/) and [official Maven](https://maven.neoforged.net/releases/).
- [CustomSkinLoader by xfl03 and contributors](https://github.com/xfl03/MCCustomSkinLoader), distributed unmodified from the [author's Modrinth project](https://modrinth.com/mod/customskinloader). Source and license remain with that project; no source was copied into Sporium. Pinned metadata is in `src-tauri/skinmod.json`.
