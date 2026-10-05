# Pack import and export — phase 11

**Импортировать / Import** is available on Home and in the local library. Choose a
`.mrpack` or `.sporium`, drop one supported file onto the main/import window, or
open it using Windows **Open with Sporium**. Each path opens a preview; confirmation
creates a NEW UUID instance. A package never merges into the open instance.

The preview shows the name, pack version, exact Minecraft and loader version,
required downloads, embedded size and individually selectable optional client
files. Java/Minecraft base installation happens through the existing game workflow
on first launch. Provider modpacks in the Modrinth catalogue also open this preview.
Game/account selection remains global; source launcher accounts and launch hooks
are never transferred.

## Modrinth `.mrpack`

Implementation follows the current [official Modrinth specification](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack).
Accepts ZIP archives with root `modrinth.index.json`, format version 1, Minecraft,
and exact Vanilla/Fabric/Forge/NeoForge dependencies. Unknown dependencies,
multiple loader families and Quilt are refused rather than guessed.

- Both SHA1 and SHA512 and the exact declared size are checked for downloads,
  including cache hits. Downloads use the existing resumable verified artifact cache.
- The current pack download allowlist is **HTTPS cdn.modrinth.com only**, with the
  existing checked redirect policy. Packs with other download hosts are not supported.
- Client `required` files are installed; client `optional` files are opt-in;
  client `unsupported` files are skipped. Dedicated-server semantics are respected.
- Shared `overrides` replace downloaded payloads; `client-overrides` replaces shared
  overrides. Server overrides are not installed. Overridden downloads are omitted.
- Complete archive path preflight rejects traversal, absolute/ADS/device paths,
  case aliases, links, special entries and file/parent collisions. Archive names
  cannot modify launcher internals, game libraries or authentication stores.
- Accepted payload roots: mods, mods_disabled, config, defaultconfigs, saves,
  resourcepacks, shaderpacks, kubejs, scripts, datapacks; selected game option files
  and servers.dat. Unknown payload roots are unsupported. Extra archive metadata
  outside override layers is reported under Details and ignored.
- Limits: 500 MB source ZIP/per file, 2 GB combined declared payload/overrides,
  10,000 archive/source entries, 12 MB manifest. Preview expires after 15 minutes.
- Freeze source bytes, extract privately, verify before publishing. Cancel/error
  before publication leaves the library unchanged. Publishing uses the existing
  instance journal and atomic directory rename. Abandoned marked private stages
  are cleaned on the next import; active preview leases prevent their removal.
- Persist the source manifest, selected files, override hashes and source/time in
  `.sporium/import.json`. Catalogue imports retain the verified pack project,
  version and archive hash. Installed artifacts receive Modrinth receipts only
  after exact API hash/file/size/SHA1 and compatible project identity proof;
  unavailable/unknown matches remain untracked with their original pack manifest.
  No archive-supplied receipt is trusted.

## `.sporium` schema 1

Compact ZIP format, root `sporium.index.json`:

```json
{
  "schemaVersion": 1,
  "manifest": {
    "formatVersion": 1,
    "game": "minecraft",
    "versionId": "example-1",
    "name": "Example",
    "summary": null,
    "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.14" },
    "files": []
  },
  "overrides": {}
}
```

`manifest.files` uses the `.mrpack` reference structure: path, both hashes,
downloads, fileSize and optional environment. `overrides` maps every embedded
relative path to `{ "sha512": "…", "size": 123 }`. The corresponding bytes live
at `overrides/<relative path>`; missing, extra or changed embedded files fail import.
New schema versions/unknown top-level fields are refused. No executable launch
commands, authentication fields or absolute runtime paths exist in this schema.

**Экспортировать / Export** appears in the instance menu. Export acquires an
exclusive instance lease and writes atomically to a NEW `.sporium` file; existing
packages are never overwritten. Exact unmodified tracked Modrinth files become
references. Configuration/options are embedded. Worlds are opt-in. Local,
modified and otherwise unreferenced mods/resource packs/shaders are excluded by
default, reported and only embedded when the user selects the redistribution-rights
option. Minecraft/Java/assets, launcher internals, accounts, logs/screenshots and
private authentication data are excluded. The source instance is preserved.

## External launchers

Select **Из другого лаунчера / From another launcher**, then a supported instance
directory or launcher data/instances directory. Close the source launcher and
game first. After selecting a detected candidate, files are privately copied and
the ordinary preview/confirmation is shown. Original files are never moved,
renamed, deleted or rewritten; file hashes and metadata proofs bind the copy.

Supported conservative adapters:

| Source                      | Understood metadata / limits                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Prism / MultiMC             | `mmc-pack.json` format 1 + `instance.cfg`; one `.minecraft` or `minecraft` game directory, exact known components. Custom patches, jar mods and unknown loader components are refused.                                                                                                                                                                                                |
| ATLauncher                  | Modern `instance.json`, `launcher.name`, vanilla `id` or loader `inheritsFrom` and known `loaderVersion.type/version`; game files in the instance root.                                                                                                                                                                                                                               |
| Official Minecraft Launcher | `launcher_profiles.json` profiles with a concrete version and local self-contained vanilla version JSON. Relative gameDir, unresolved `latest-*` and custom inherited/modified recipes are unsupported.                                                                                                                                                                               |
| Modrinth App                | Closed `app.db` with current instances/content-set tables or legacy profiles schema, installed concrete game/loader and one safe relative profile name. Read a private database copy; never create source WAL/SHM. Active WAL is refused. Default data/profile location is supported; custom split database/data locations require a supported metadata adapter rather than guessing. |

Copied categories: understood mods/configs/worlds/resource packs/shaders/options.
ATLauncher `disabledmods` and Modrinth/Prism `mods/*.jar.disabled` map to Sporium's
`mods_disabled/*.jar`, with collision checks and source bytes preserved.
Omitted top-level items are listed under Details. Unknown formats/arbitrary
directories and generic ZIPs never masquerade as supported packages. There is no
CurseForge provider import or authentication/token migration.

Format evidence: [MultiMC export layout](https://github.com/MultiMC/Launcher/wiki/Export-Instance),
[Prism instance source](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/MinecraftInstance.cpp),
[ATLauncher instance source](https://github.com/ATLauncher/ATLauncher/blob/master/src/main/java/com/atlauncher/data/Instance.java),
[Modrinth instance schema](https://github.com/modrinth/code/blob/main/packages/app-lib/migrations/20260611120000_instances-content-foundation.sql).

## Windows opening

The import dialog's **Добавить Sporium в «Открыть с помощью»** registers
`HKCU\Software\Classes\Sporium.Pack\shell\open\command` with the current launcher
executable and a quoted `%1`, and `Sporium.Pack` OpenWithProgids entries for
`.sporium`/`.mrpack`. This is an explicit user action; no generic ZIP/default
association is hijacked. A restarted application receives a supported absolute
file path, consumes it once, and requires preview/confirmation. Installation and
uninstall association ownership will be integrated with the later installer phase.

## Verification

Core pack tests cover schema/loader constraints, URLs, traversal/case collisions,
hashes/cache, override/environment semantics, cancellation/tampering, round-trip,
export omissions/no overwrite, external adapters, disabled files, active WAL,
junction refusal and private-stage recovery. Existing instance publication recovery
tests cover both sides of the atomic directory rename.

`npm run test:pack-native` uses isolated `.local/pack-native-smoke` data and real
WebView2/IPC. It exercises the file-open argument, UI previews, optional selections,
cancel/corruption, world preservation, external copy, restart and `.sporium`
round-trip. It installs the real Fabulously Optimized 6.5.0 pack for Minecraft
1.21.1, checks provider receipts, exports/reimports references, then launches the
imported game to renderer/audio and stops it cleanly. Synthetic world bytes in
earlier fixtures prove copy/round-trip integrity, not world playability.

Windows OS picker/drop and Registry UI activation are not simulated by the native
suite. The standard Tauri drop API is shared with the previously user-confirmed
local content workflow; the actual association command argument is exercised.
