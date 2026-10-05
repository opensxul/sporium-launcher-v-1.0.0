# Managed projects and update safety (phase 12)

Open an instance and use **Управление сборкой**. **Создать проект** captures its current files as a versioned Creator Studio project. Review the name, version and individual file policies under Details, then preview and confirm. The template uses actual instance contents; it does not hard-code a mod list. Local originals are retained on this computer for repair.

Manifest schema 1 records project identity/version, exact Minecraft/loader/version, SHA256 and SHA512, sizes, optional Modrinth CDN sources, groups and bounded launch memory. Arbitrary JVM arguments, executables, credentials and launch hooks are not accepted. `memoryMib` is a launch preference bounded by available memory and legacy Java limits.

- `REQUIRED_LOCKED`: required bytes, protected from ordinary content mutation and checked before Play.
- `OPTIONAL`: installed for selected groups; subsequent removal/replacement is shown in the preview.
- `USER_ALLOWED`: existing user bytes are retained; a missing declared default can be restored.
- `USER_FORBIDDEN`: installation is refused; existing declared files require explicit removal confirmation.

“Forbid external mods” restricts additional JARs. A project can still be edited deliberately through Creator Studio. Such editing creates a restore point. Managed projects remain in the common library and use the global nickname.

**Восстановить файлы проекта** prepares verified replacements. **Открыть манифест проекта** selects a local JSON source; the exact file is bound to confirmation and can later provide version updates. Export writes JSON to a new file. Local-only payloads remain in this computer's private source store; exporting the JSON alone does not transfer them to another computer. Portable content transfer uses the existing `.sporium` export.

Official Modrinth pack provenance enables whole-pack update checks and repair. New archives use the existing bounded, verified pack pipeline. Optional components are previewed as groups and existing selections are retained where paths match. Preparation may download optional artifacts to establish their hashes; only selected components are published. User worlds, screenshots and unrelated files remain untouched. Required dependency diagnostics are checked against the candidate mod set; unsupported metadata requires manual review.

Updates require the same exact Minecraft and loader recipe. A release requiring another recipe must be imported as a separate instance. This protects existing worlds/configuration from an implicit game-version migration. Stable channels stay stable. Per-content pins and ignored versions prevent whole-pack changes to affected artifacts; the user must clear the exception before applying such a pack change.

## Automatic updates

Global controls are in **Settings → Catalog**; instance controls are under **Управление сборкой → Автоматические обновления**. Content and whole-project modes are independent: Off, Check, Install; each instance can inherit or override either mode. Defaults only check. Enabling installation requires a visible consent checkbox and Save.

One background worker per data directory begins after approximately 30 seconds, checks idle instances approximately every 30 minutes and respects operation leases. Check Now uses saved policies. Compatible automatic changes use the same verified previews, dependency checks and restore points as manual changes. Modified files, forbidden-file removal and incomplete dependency metadata require manual review. A source outage is reported separately and never turns off local Play. Changes to policy are rechecked before starting an automatic mutation.

## Recovery and history

Project mutations freeze before/after blobs and configuration in `.sporium/project-snapshots/<id>`, verify the entire batch, then publish a durable `.sporium/project-change.json` journal. Reads and game operations recover interrupted publication before proceeding. Snapshot bytes live inside the instance and are not removed by download-cache trimming. Bounds are 500 MB per content file and 2 GB per transaction; configuration backup is bounded separately.

**History → Restore points** includes content and project points. Restore first creates an undo point. Optional restoration of saved user settings requires explicit selection; required locked project configuration follows the restored manifest. Changed unrelated files are not overwritten, and world data/screenshots are not rolled back. Changed game/loader recipes, corrupt snapshots, incompatible project history and destination conflicts refuse restoration. A content-only point cannot bypass locked project files. Older content snapshots without verified configuration hashes remain usable for content restoration only.

Verification scripts: `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests`, `node scripts/project-native-smoke.mjs`, `npm run test:restore-native`, `npm run test:native`. Native scripts use fresh `.local` data roots; they do not modify the normal user library. See `PROJECT_STATE.md` for verified results and remaining work.
