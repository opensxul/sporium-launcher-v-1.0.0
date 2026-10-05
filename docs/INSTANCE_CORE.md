# Instance core — phase 2 / v2.5

Existing phase 0/1 behavior and settings are retained. SQLite migrates from version 1 to 2 without replacing settings or their revisions. Instance fields added for v2.5 have backward-compatible defaults; reading an older payload does not rewrite it.

## Storage and recovery

- SQLite is authoritative for names, folder membership, favorites and versioned metadata. `instance.json` is an immutable UUID/schema identity marker, not a second mutable metadata store.
- Names never become directory names. Only validated canonical UUIDs select managed paths. Each instance has separate mods, disabled mods, configuration, saves, resource packs, shader packs, screenshots and logs.
- Rename and folder assignment change metadata, not directory paths. Deleting a user folder detaches its instances transactionally and retains their files. Stale revisions are rejected.
- One OS file lock serializes instance operations across launcher processes. A busy library returns a localized retryable error.
- Creation/copy: persist a building journal record, create staging, copy independent bytes, mark publish, rename into place, then commit the record and clear the journal. Recovery discards unfinished staging or finishes a publish, depending on its persisted phase.
- Deletion: validate the tree, journal intent, detach into managed trash, remove the catalog record, then purge or move into backups. Preserve mode retains `saves`, `screenshots`, identity and complete backup metadata. Journal recovery resumes after an interruption. An interrupted cleanup may leave managed staging/trash until the next successful recovery.
- Path checks reject traversal, symlinks and Windows reparse points/junctions; recursive deletion stays beneath managed paths. Copy verifies source timestamps/length and never hard-links instance data. These checks protect ordinary launcher operations; they are not a sandbox against another process actively racing filesystem changes with the same Windows privileges.

## v2.5 additions and gates

- Favorites are real persistent metadata with UI controls, home display and library filtering. No fictional play history is generated.
- Icon source is typed (`automatic`, `builtin`, `custom`), with an optional reference. The approved builtin catalog is intentionally empty pending user-supplied artwork. Automatic selection uses the random instance UUID, stays stable across restarts and selects only from that catalog. Tests use names as fixture data, not substitute artwork. The final icon picker, uploads, catalog population and reset UI belong to phase 14.
- Managed shortcut metadata is optional. `instance_shortcut_plan` returns UUID-based arguments `--launch-instance <uuid>` with `available: false`. Renames cannot break the identity; duplicates never inherit managed shortcut ownership. This command does not create a `.lnk` or claim that quick launch works. Actual Windows shortcuts, icon conversion, rename/icon refresh and single-instance launch routing are open integration items for phase 16 after the launch/account core exists.
- New instances start as `not_installed`. Phase 3 now supplies the official version picker, Vanilla installation, Java resolution and real process lifecycle; see `VANILLA_LAUNCH.md`. Other loader installation remains in its respective phase. Running/installing instances hold a file lease that blocks deletion and duplication.

## Verification

24 Rust tests cover settings plus filesystem isolation, preservation, duplicate independence, junction rejection, lock contention, additive metadata, stable shortcut arguments and simulated interruption recovery. Frontend checks cover the read-only preview and existing routes/settings. Native smoke uses real Tauri IPC, a temporary local data root and the built executable; see `.local/native-smoke/result.json` for the last completed run.

Logo integration and actual Windows shortcut creation remain explicitly open; the core implementation must not be represented as completion of those later requirements.
