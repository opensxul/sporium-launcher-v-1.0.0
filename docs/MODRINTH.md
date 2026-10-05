# Phase 9 — Modrinth

## Scope and user flow

The instance page now puts **Installed content** directly below the game controls. Its local search, type chips and name/version-date sorting work on the installed receipts. **Add content** opens a compact dialog above that same instance instead of navigating to the global catalog. The dialog has Mods, Resource packs and Shaders tabs, search, category/sort filters, pagination, verified Installed badges and a Hide installed toggle. Hidden results are filtered on the current provider page; the empty state offers showing installed content or moving to another page. The target Minecraft version and loader remain locked by the backend.

Selecting Install opens the project's compatible versions and dependency plan above the catalog dialog. Confirming starts the existing verified installer and closes only the project dialog; the catalog preserves its search/type/page and displays progress. Installed badges and the underlying content list refresh when the job finishes. Closing the catalog returns focus to Add content. Escape closes only the topmost dialog. While a game or installation owns the instance, browsing is available but installation controls are disabled. This does not add the later bulk/update/enable/disable features.

The instance view remains mounted during background library snapshot refreshes. Receipt verification can briefly hold a shared file lease while a dependency plan or installation needs an exclusive lease; the frontend retries only `INSTANCE_BUSY`/`LIBRARY_BUSY` up to twelve times, 250 ms apart. Other errors are reported immediately, and the backend's token, compatibility and file-conflict checks remain authoritative.

The global **Catalog** uses Modrinth's official v2 API. It supports text search, pagination, type, Minecraft version, loader/engine, category, environment and provider-supported sorting. Categories, loaders and game versions come from provider tags. An instance's **Add content** link opens a locked compatibility context. The Rust backend derives that context from the stored instance, regardless of caller-supplied filters, and checks actual versions and downloadable files before displaying a contextual result.

Project details show the author's description as plain text, license information and an explicit link to Modrinth. Global installation offers only compatible instance destinations; selecting one reloads that instance's compatible file versions. Before installation, a plan lists the main file and required dependencies, versions, filenames, total size and existing verified files. Optional dependencies are counted but not automatically installed. The user confirms this concrete plan.

The 2026-09-30 extension adds **Create a new instance** directly to the destination selector for mods and resource packs. Choose a project version, one of its declared Minecraft versions and a supported loader (Fabric, Forge, NeoForge, or Vanilla for resource packs). The backend validates the official game/loader catalogs and the complete required dependency plan before creating anything. Closing the dialog or a compatibility error leaves the library unchanged. Confirmation creates one isolated instance named by the user and stores the originating project's icon. The same prepared token cannot create another instance on retry.

After publishing the verified content, an uninstalled destination automatically prepares Minecraft, Java and its loader through the existing game installer. The instance lease remains held across both stages. The Downloads page shows content and game progress; cancel works during either downloading stage. The game does not launch automatically. A failed/cancelled installation retains the new instance and verified content; preparation can be retried from that instance, with interrupted game operations using the existing explicit-resume mechanism. Already installed destinations keep their version and loader. Shader archives still target an existing instance with a separately installed engine; whole modpack import remains phase 11.

Project icons come from the official API's `icon_url` field and are shown in search results of all supported kinds, project details, dependency plans, installed content and instances created from a project. The UI accepts only HTTPS URLs on `cdn.modrinth.com`, uses image-only CSP access, sends no referrer, and shows a type-specific symbol if the icon is absent or fails. Existing receipts without an icon remain readable and can resolve one from cached/provider project metadata. Icon loading is cosmetic and never blocks installation or local play. Arbitrary local JAR icon extraction and custom instance artwork remain separate content/artwork work.

Supported destinations are mods (`mods`), resource packs (`resourcepacks`) and Iris/OptiFine shader archives (`shaderpacks`). Shaders still require the appropriate engine, and resource packs/shaders must be enabled in-game. Modpacks are browsable globally; whole-pack import remains phase 11 and will create a new instance. Datapacks, maps and plugins are not offered without a supported world/server destination. Single/bulk mod enable/disable and confirmed content deletion work with history. Local JAR import, manual-file inventory, explicit adoption with reliable hash matching/local icons and manual compatible updates are implemented. Richer local dependency inspection and world destinations remain in phase 10. See `CONTENT_MANAGEMENT.md`.

## Provider and compatibility

`ContentProvider` separates search, projects, version files, individual versions and tags from the rest of the launcher. The Modrinth implementation identifies Sporium in its User-Agent, serializes API requests with minimum spacing, honors rate-limit reset headers and bounds response sizes and timeouts. It does not scrape pages or require credentials for public content. API responses are cached in `shared/cache/modrinth`: searches and version lists for five minutes, project/version details for fifteen minutes and tags for one hour. Cached searches/tags can be used during an outage; installation metadata errors remain explicit. The local library remains independent of provider availability.

Compatibility requires the exact Minecraft version and loader on the selected version. Forge and NeoForge remain distinct. Resource packs require the `minecraft` loader; supported shader files declare Iris or OptiFine. Dedicated-server-only versions are rejected for these client instances. For older unknown environment metadata, the provider's legacy client-side field is retained as a fallback. Contextual search checks version metadata instead of excluding valid packs solely for an unknown project-level environment. Provider declarations do not prove that an arbitrary combination of local mods works together.

The bounded dependency traversal follows required project/version references, validates pinned versions, deduplicates projects and cycles, and checks incompatible declarations in both directions against installed managed content. Existing project versions are reused only when compatible. Conflicting pins or versions are reported; the installer does not silently upgrade/downgrade projects or attempt arbitrary dependency solver backtracking. External required files without resolvable provider identifiers are rejected.

## Installation and persistence

Only a short-lived, server-held plan token can start an installation. Confirmation rechecks the instance and installed-content receipt, acquires its file lease and the shared download lock, then verifies destination conflicts. Unknown local files, modified managed files, disabled files with the same destination name and different installed versions are preserved. A stale plan must be rebuilt after another content installation.

Files use the phase-8 downloader with resumable parts, bounded retry/backoff and SHA-512 verification; SHA-1 is also checked against Modrinth metadata. Stable content-addressed cache destinations permit reuse across instances and retries. Each installation stages and flushes all files inside the instance before applying anything. A durable pending journal supports recovery after interruption; each final file is published atomically without overwriting an existing destination. The instance lease prevents launch, duplication or deletion during installation. Launcher close is blocked until the content operation finishes or is cancelled. Cancellation before application preserves existing instance contents; application completes its short journaled transaction.

`.sporium/content.json` preserves provider, immutable project/version IDs, file ID when supplied, declared hashes, source URL, filename, destination and dependency metadata. The instance view reports verified, missing or modified files. No receipt is inferred from a similar local filename. `.sporium/content-pending.json` is recovered under the instance lease before its next launch or content access. Recovery validates all staged files before publishing further files and requires no network. Unreadable or damaged journals remain in place and block unsafe continuation.

Modrinth response and artifact caches participate in the existing disposable-cache controls. Installed mods/packs and the installation receipt are inside the instance and are never removed by cache cleanup. The global active nickname and shared instance library are unchanged.

## Verification and reproduction

Rust tests cover exact version/loader/environment checks, dependency cycles and deduplication, optional and incompatible dependencies, pinned version conflicts, unsafe filenames and foreign download hosts, changed receipts invalidating a prepared plan, preservation of local files, partial-publication recovery, corrupt staging and provider cooldown/cache fallback.

The native scenario `scripts/modrinth-native-smoke.mjs` creates a new isolated UUID instance under the existing `.local/modded-smoke` test root, reusing only its shared game caches. It exercises real WebView2 IPC, global search, compatible destinations and versions, the dependency plan, installation, SHA-512 receipts, unchanged local/world sentinels, locked context, repeat-install checks, local/modified file conflicts, restart persistence and an actual Fabric launch. It also installs a resource pack and shader into their distinct directories. The script never accesses normal player data.

```powershell
rtk proxy npm.cmd run desktop:build
rtk proxy npm.cmd run test:modrinth-native
```

The scenario requires the previously created Fabric 1.21.1 fixture caches. Run native tests sequentially after compilation; they share WebView2's browser profile. Results and screenshots are saved under `.local/modrinth-native-smoke/`. The extended scenario checks that real CDN icons decode, planning leaves the library unchanged, confirmation creates and prepares the destination automatically, and icons and receipts survive restart.

The final embedded desktop build passed all six native Modrinth checks on 2026-09-28:

1. Official global search, compatible destination/version selection and an explicit three-file dependency plan.
2. Verified downloads and SHA-512 receipts, with existing local files and a world sentinel preserved.
3. Backend-enforced instance compatibility, repeat-install reuse and protection of modified managed files.
4. Refusal to overwrite or adopt an unknown local file with the same filename.
5. Receipt persistence after restart and actual Fabric 1.21.1 renderer/audio initialization with the installed mods.
6. Compatible resource-pack and shader installation into separate destinations, including legacy environment metadata in contextual search.

| Content                                    | Installed version | Destination     |
| ------------------------------------------ | ----------------- | --------------- |
| Mod Menu                                   | 11.0.5            | `mods`          |
| Fabric API (required dependency)           | 0.116.17+1.21.1   | `mods`          |
| Text Placeholder API (required dependency) | 2.4.2+1.21        | `mods`          |
| Faithful 32x                               | 1.21.1            | `resourcepacks` |
| Complementary Reimagined                   | r5.9.3            | `shaderpacks`   |

The authoritative result is `.local/modrinth-native-smoke/result.json`, with the game log path and immutable provider version IDs. This verifies installation and the modded client's renderer/audio startup; it does not claim in-world gameplay or enabled shader rendering.

On 2026-09-30 the extended scenario passed seven checks on the rebuilt desktop executable. It used the same content versions above, creating instance `2dd0f58f-e1ce-485c-8dc5-c56a8b4d059e` entirely from the Mod Menu dialog. Planning left the library byte-for-byte unchanged; confirmation persisted the selected game/loader and project icon and prepared Minecraft 1.21.1 with Fabric 0.19.5. Real catalog, instance and installed-mod images decoded successfully. No game was launched until the separate launch check after restart, which again reached renderer/audio initialization. The existing dependency, collision, preservation and resource-pack/shader checks also passed. Unit coverage additionally verifies old receipts without icons, read-only invalid/incompatible creation plans and cancellation before game preparation without a game process.

The later instance-dialog extension passed all nine native checks on 2026-09-30 at 11:40. Instance `307c0baf-0eb2-4216-9576-ce637511b96a` received AppleSkin 3.0.6+mc1.21, Faithful 32x 1.21.1 and Complementary Reimagined r5.9.3 through the actual nested dialog UI. Every installation retained the instance route and search, updated its Installed badge, and refreshed the local list to six verified files. The suite checks hide/show installed projects, locked context, topmost-only Escape, focus restoration, local text/type filtering, restart persistence and Fabric renderer/audio startup before the additional content. All 15 baseline native instance/settings checks also passed. The authoritative result now points to this final instance; earlier instances above are historical test runs. No claim of in-world AppleSkin behavior or enabled shader rendering is made.

## Official sources audited

- [API overview, authentication, user agents and rate limits](https://docs.modrinth.com/api/)
- [Project search and supported facets](https://docs.modrinth.com/api/operations/searchprojects/)
- [Project versions, dependency references, files and hashes](https://docs.modrinth.com/api/operations/getprojectversions/)
- [Project details and distribution metadata](https://docs.modrinth.com/api/operations/getproject/)
- [Environment semantics](https://modrinth.com/news/article/new-environments/)

The live v2 API was also checked for actual project/version environment fields, dependencies and file identifiers. This uses public provider content, with no redistribution or authentication workarounds.

## v3.0 plugin provider preparation

Plugin discovery uses the official `all_project_types:plugin` facet and Paper/Purpur categories. The provider can select downloadable plugin versions for an exact Minecraft version and explicitly declared Paper or Purpur platform, rejecting unsupported environments, foreign IDs, unsafe files and client destinations. No inferred Bukkit/Spigot/Paper/Purpur compatibility is applied. A live API test searched for a real Paper 1.21.1 plugin and resolved a compatible JAR. This closes the provider preparation requirement only; server catalog UI, installation, plugin dependencies and real server launches remain phase 13. Run the explicit live check with `rtk proxy cargo test --manifest-path src-tauri/Cargo.toml live_plugin_discovery_and_version_selection -- --ignored`.

## Latest content-management verification

On 2026-09-30 at approximately 16:45 the extended native scenario passed all 12 checks on instance `564caddf-1dc2-4142-aa52-d1b7a42086e6`. The original nine checks still passed, including real Fabric renderer/audio launch. It additionally cancelled then confirmed a bulk disable of AppleSkin and Mod Menu, checked actual file relocation and provenance, restarted the launcher, re-enabled both mods and deleted only AppleSkin after confirmation. Ordered history persisted throughout. This fixture/result supersedes the earlier instance IDs above. No normal user data was changed.

After correcting wide-window toolbar spacing and hiding the completed content banner, the final rebuilt EXE passed four targeted UI checks (single toggle, history, 1400/1000-pixel layouts, no horizontal overflow) and all 15 baseline native checks. The final UI evidence is `.local/modrinth-native-smoke/final-ui-result.json` and `management-1400.png` / `management-1000.png`. Full network/game verification preceded only those two cosmetic changes. All 68 offline Rust tests and the separate live plugin test passed; Clippy, generated bindings, frontend checks and the desktop build passed. Phase 10 remains in progress; see `CONTENT_MANAGEMENT.md` for its remaining work.
