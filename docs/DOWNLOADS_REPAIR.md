# Phase 8 — downloads, cache and repair

Implemented against specification v2.5, section 20. Existing global profiles, instance library and loader support remain in place; no data reset is required.

## Using the launcher

- **Downloads** shows the current preparation operation: file queue, active filenames, verified progress, transferred bytes, speed, estimated remaining time, cache hits, retries and repaired files. Pause, resume and cancellation are available while the game file queue is running.
- **Settings → Downloads** saves the number of concurrent file downloads (default 6, supported range 1–12). A saved change applies to the next queue.
- A paused download permits closing the launcher when no game is running. On reopening, the interrupted operation offers an explicit continuation. Nothing launches automatically on startup. Choosing **Retry launch** explicitly repeats the prior launch action with the currently active global nickname.
- **Install / repair** verifies managed game files and downloads missing or changed files again. Modified or damaged files repaired by the file queue are counted together: a checksum cannot determine whether a change was intentional. User mods, configuration and worlds are preserved and are outside this repair scope.
- **Settings → Storage** shows disposable cache size, partial download size, confirmed cleanup and an optional automatic size limit. The default is unlimited. After successful preparation, old disposable files are removed first until the limit is met.

## Transfer and recovery guarantees

Artifacts with a known digest and size use persistent `shared/cache/parts/*.part` files. Their identity includes the expected hash, size and destination. Existing installed files are verified before reuse. A failed download never replaces the installed target; a complete part must pass its size and SHA-1/SHA-256 checks before atomic publication through a temporary file in the destination directory.

After interruption, a new request uses HTTP Range. A 206 response must describe exactly the requested range and expected total. A server that returns 200 instead restarts the part safely. Invalid ranges are rejected; oversized or complete corrupt parts are discarded before retry. A transfer has up to three attempts with cancellable backoff. Incomplete parts survive cancellation and process exit. Requests have bounded timeouts, so pause/cancel may wait for an in-flight network read to finish or time out.

`launcher/download-operation.json` stores the interrupted operation, not credentials or an automatic launch instruction. Successful completion and cancellation remove it; an interrupted or failed operation can be explicitly retried. Recovery re-resolves the operation and verifies cached files before resuming partial artifacts. Already cached immutable version metadata can be used without refreshing the whole catalog; explicit catalog refresh remains available.

The file queue is within one installation/preparation operation. Multiple instance installations are still serialized. Java provisioning and official Forge/NeoForge installers are separate preparation stages: they support cancellation, but the game file queue's pause control does not pause those external installers. Artifacts fetched by Sporium use resumable transfer when their expected size is known; a loader artifact whose server supplies no usable size retains the bounded checksum-verified fallback. The official installer controls its own internal downloads.

## Cleanup boundaries

Disposable cache directories are `shared/cache/parts`, `shared/cache/java` (downloaded Java installation archives), `shared/cache/skins` and `shared/cache/loaders`. Cleanup keeps installed Java runtimes, clients, game assets, libraries, version metadata and every instance directory. Lock files are excluded. Checked paths reject links/reparse points, and cleanup takes the shared download lock, including when an installation is paused.

Phase 9 additionally includes `shared/cache/modrinth` API responses and verified artifact copies in disposable cleanup. Installed content and its provenance receipts remain inside each instance and are preserved. See [Modrinth](MODRINTH.md).

The limit applies only to these disposable caches, not the full game library. Clearing partial downloads intentionally discards their saved download progress.

## Verification

Rust regression tests exercise connection loss and successful Range continuation, cancellation across a new client, a server ignoring Range, rejected Content-Range, corrupt payloads preserving the old target, repair after failure, and HEAD size discovery with an empty response body. Recovery tests verify that restart never launches a game and cancellation releases paused workers. Cleanup tests verify the shared lock and preservation of clients, libraries and worlds.

The native scenario uses the actual Windows executable and real IPC in the dedicated `.local/game-smoke` fixture. It saves concurrency through the UI, pauses, rejects cleanup during installation, restarts, explicitly resumes, repairs a corrupted managed logging file and checks byte-for-byte preservation of a test mod and world. It then saves a cache limit and clears disposable data through the UI. The verified operation checked 5,223 files, reused 5,222 cached files and repaired one changed file.

```powershell
rtk proxy npm.cmd run desktop:build
rtk proxy npm.cmd run test:download-native
```

This scenario requires the existing Minecraft 26.3 game-probe fixture; it never uses the normal player library. Results and screenshots are under `.local/download-native-smoke/`. Run native suites sequentially, after compilation, because Windows locks running executables and WebView2 shares a browser profile. Network fault injection uses a loopback HTTP server only in Rust unit tests; production downloads retain the approved HTTPS host and redirect policy.
