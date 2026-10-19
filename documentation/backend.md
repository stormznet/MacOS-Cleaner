# Rust backend and native bridge

[Documentation index](README.md) · [Command and model reference](api.md)

The backend owns filesystem authority. UI commands supply scan IDs or history IDs; they do not supply arbitrary cleanup paths. This guide follows the implementation in [src-tauri/src/](../src-tauri/src/).

## Startup and command dispatch: main.rs

`main()` builds the Tauri application with its command handlers, initializes `AppState` with Tauri's home/application-data paths, and then runs the event loop. State initialization errors return from `main()` before the event loop starts, so an already-running instance or unreadable history fails startup without panicking in Tauri's setup hook. There is no dedicated recovery window for an unreadable history file.

The command handlers are thin adapters around scanning, cleanup, persistence, and native integration. Scan, Trash, and restore run through `tauri::async_runtime::spawn_blocking`. The full parameter/result contract and individual handler behavior are in [the IPC reference](api.md#commands).

## Models, identity, and persistence: model.rs

| Symbol | Responsibility |
| --- | --- |
| `now()` | Current Unix timestamp in seconds; used in history |
| `operation_id()` | Process ID plus current nanosecond timestamp; used for scan, batch, and record IDs, not a secret or authentication token |
| `Identity::from()` | Captures device, inode, modification seconds, and modification nanoseconds from metadata |
| `Identity::same_file()` | Compares device and inode only, used after moves and during recovery |
| `Item`, `Record`, `Settings`, `Saved` | Serializable scan, history, settings, and persisted-state models |
| `Store` | Saved state plus the in-memory item registry |
| `AppState` | Home/data paths, store mutex, busy/cancel flags, and held state-file lock |
| `AppState::new()` | Creates/checks the data directory, locks it, and loads existing state or defaults |
| `AppState::save()` | Writes and syncs a temporary JSON file, renames it over the state file, then syncs the directory |
| `AppState::start()` / `BusyGuard::drop()` | Acquire the operation flag with compare-and-exchange; release it even when an operation returns an error |
| `path_identity()` | Uses `symlink_metadata`; rejects a symbolic link at the inspected path |
| `clean_ancestry()` | Requires an absolute path without `..`, then rejects symlinks or inaccessible components along its ancestry |
| `is_node_modules_name()` | Recognizes the exact dependency name, a space plus ASCII digits, or a space plus `HH-MM-SS-mmm`; shared by discovery and root exclusions |
| `validate_root()` | Canonicalizes a chosen project root and enforces directory, home, excluded-component, ancestry, and device rules |

`Identity` has two comparison modes. A scan-to-cleanup comparison uses all four fields to catch a replaced or top-level-modified candidate. A post-move/recovery comparison uses device and inode because movement can alter metadata. Neither comparison hashes content or snapshots descendants.

`AppState::new()` sets the app-data directory mode to `0700`. Lock and temporary-state files are created with mode `0600` and `O_NOFOLLOW`. The state lock is nonblocking and exclusive; an already-held lock fails startup. Invalid JSON is preserved and reported rather than replaced by empty history. The schema has no version or migration layer.

Project roots are canonicalized before ancestry checks: a selected alias/symlink path can resolve to an allowed real directory. Discovery itself does not follow symbolic-link directory entries. The precise root exclusions are documented in [Settings](features.md#settings-and-appearance).

## Discovery and measurement: scan.rs

### Helper functions

| Symbol | Behavior |
| --- | --- |
| `ScanEvent` | Tagged `item`, `phase`, `warning`, and `progress` messages for the frontend channel |
| `Summary` | Final elapsed time, cancellation flag, candidate count, and warnings |
| `placeholder()` | Validates ancestry, captures identity, and constructs an incomplete item with zero initial bytes |
| `children()` | Lists immediate children; missing directories are normal, other listing errors become warnings |
| `hidden_or_bundle()` | Recognizes leading-dot names and `.app`, `.photoslibrary`, `.bundle` suffixes for discovery exclusions |
| `valid_bundle_id()` | Requires at least one dot and nonempty ASCII alphanumeric/hyphen segments before constructing associated-data paths |
| `CacheApp`, `cache_owner()` | Index app scan IDs and names by bundle ID; match exact IDs, ShipIt IDs, or unique ASCII-case-insensitive name aliases without arbitrary prefix matching |
| `discover_app_data()` | Inspect fixed user/system Library locations, link exact bundle-ID or unique-name matches to their app, and report rejected candidate paths |
| `discover_nodes()` | Iterative directory traversal under one root, stopping at dependency candidates and excluded subtrees |
| `measure()` | Metadata-only recursive allocated-block measurement, with cancellation and device/placeholder checks |
| `measure_items()` | Runs four candidate-measurement workers, updates the registry, emits item/progress events, and returns measurement warnings |
| `run()` | Coordinates all discovery, four-worker measurement, registry updates, and events |

### Scan sequence

`run()` clears the old registry and copies configured project roots. An empty root list adds a warning that node_modules was not scanned, with directions to add a folder. A path set deduplicates candidates across discovery sources. Adding a candidate immediately emits an incomplete `item` event; the candidate enters the authoritative registry only after its measurement attempt.

Discovery runs in this order:

1. Immediate children of Xcode DerivedData.
2. Four fixed package-cache locations.
3. App bundles in the two supported application roots, then associated data in fixed user/system Library locations for eligible non-Apple apps. Resolving names after the full inventory prevents assigning aliases shared by multiple apps.
4. Immediate children of the user cache directory, matched against installed apps, registered macOS applications, or the special Xcode identifier.
5. Validated user project roots, searched for dependency directories.

App metadata comes from the native `Info.plist` reader. `CacheApp` keeps the display label and aliases from the display name, bundle name, and bundle filename. `cache_owner()` prefers exact bundle IDs (including the parent ID of a ShipIt folder), then accepts a name alias only when one bundle ID matches. If no inventory match exists, a valid bundle ID is looked up through `app_name_for_bundle()`; the on-disk identifier is checked before accepting the result. Recognized caches retain the actual owning bundle ID for cleanup's running-app guard, and their descriptions show the match. Mac Cleaner's own caches remain excluded. Apple uninstall candidates are inspection-only. Associated files carry `app_parent`, the scan ID of their parent app. Unconfirmed cache owners remain visible but ineligible. See [features](features.md#supported-storage-categories) for every supported path.

`discover_nodes()` uses an explicit stack, inspects directory metadata without following links, requires the root's device, and skips the `UF_DATALESS` flag (`0x40000000`). It stops at every directory recognized by `is_node_modules_name()`, even an orphan lacking a parent manifest. This includes numbered and timestamped copies returned from Trash, without treating arbitrary name prefixes as cleanup targets. Each copy remains a separate candidate, labeled with its project and folder name. Hidden directories and bundles are excluded from discovery. Once an eligible candidate is measured, hidden files inside it still contribute to its size and move with the candidate.

### Measuring storage

`measure_items()` emits an initial progress event with zero processed and the fixed discovered total. Four scoped workers claim candidates through `AtomicUsize`. For each candidate, `measure()`:

1. Walks paths using `symlink_metadata`, not file-content reads.
2. Adds `metadata.blocks() * 512` once per `(device, inode)` within that candidate, using saturating arithmetic.
3. Traverses real directories; a contained symlink's own allocation may count, but its target is not followed.
4. Fails the candidate on cancellation, inaccessible entries, another device, or a cloud placeholder.
5. Rechecks the candidate's full top-level identity after measurement before setting `complete = true`.

A successful measurement can still be inspection-only because of category rules. A failure leaves the item incomplete and unselectable, with an issue. A top-level identity mismatch also prevents cleanup. Each worker stores the resulting item and emits an update with the same ID. If cancellation has not been requested, it increments a separate processed counter and emits progress, including when the item was inaccessible. Worker emission order may differ from increment order; consumers should retain the maximum count received. Checks aborted by cancellation do not advance that counter.

After workers finish, collected warnings are emitted and returned in the summary. `count` is the number of discovered candidates, including incomplete and inspection-only entries, not the number safe to remove.

Cancellation is cooperative. Checks exist in application/project traversal and measurement, but not around every discovery action. Workers still process remaining candidates into incomplete results after cancellation. There is no ten-second timeout or incremental discovery/measurement pipeline: measurement begins after discovery is complete.

## Cleanup: cleanup.rs

### Preflight and movement

`validate()` checks that an item is complete/selectable, strictly below its stored root, has clean ancestry, still matches its scanned identity, and is on the home directory's device. If it has a bundle ID, its owning app must not be reported as running.

`trash_selected()` accepts 1–10,000 submitted IDs. It deduplicates IDs, looks them up in the current registry, and validates the entire selection before moving any item. It rejects overlapping parent/child paths and associated uninstall items whose app is missing from the same selection. User-cache candidates can carry `app_parent` while remaining independently cleanable: when their app is selected, the same relationship keeps the cache if moving the app fails. Each cache has one candidate shared by the cache and uninstall screens.

The function sorts parentless items before associated items, assigns one operation ID, and processes candidates individually:

1. Append a `pending` record and durably save it before touching the candidate.
2. If a selected parent app failed or could not be verified, keep its associated item and record the failure.
3. Otherwise revalidate and call the native Trash operation.
4. Verify the returned Trash location against device/inode identity. A verified move becomes `trashed` and is removed from the registry; an unverified result becomes `uncertain`.
5. On an error, mark `failed` if the original still has the recorded identity; otherwise mark `uncertain`.
6. Persist the outcome before continuing.

A preflight rejection causes no moves. A later per-item failure can coexist with earlier successful moves. There is no automatic batch rollback. A history-write failure stops further processing; the UI attempts a fresh bootstrap to reconcile what happened. A native failure never falls back to permanent deletion.

### History reconciliation

`reconcile()` runs during bootstrap and before a restore. It leaves `restored` and `failed` records unchanged. For other records it:

- Recognizes a `restoring` record whose original path already has the recorded identity as restored.
- Tries the recorded Trash path under `~/.Trash`, then checks immediate Trash entries for the same device/inode.
- Marks a found item `trashed`, updating its location and clearing its error.
- Marks a still-original `pending` item `failed` because the move was interrupted before completion.
- Marks other unresolved cases `unavailable` with a Finder inspection message.

Reconciliation does not search recursively through arbitrary folders or external-volume Trash. It does not infer every manual Finder restoration; only an interrupted app restore with status `restoring` is recognized at the original path. History statuses describe observed state, not permanent guarantees.

### Restoration

`restore()` reconciles history, resolves the history-record ID, requires status `trashed`, persists `restoring`, and calls `restore_file()`. It then records `restored`, or returns the record to `trashed` with an error, and saves again.

`restore_file()` requires the source to be a descendant of the current user's `.Trash`, clean ancestry, and the recorded device/inode. The original parent must exist with clean ancestry; the destination must not be in Trash and must not already exist. A known owning app must be stopped.

The final move uses `renamex_np(..., RENAME_EXCL)`. This provides an atomic no-overwrite destination check, including a conflicting destination created after preflight. There is no overwrite, cross-volume copy, missing-parent creation, or permanent-delete fallback. Parent-directory replacement races and descendant writers are not eliminated by this design; see [safety boundaries](safety.md).

## Native integration: native.rs and macos.m

| Rust wrapper | Native function | Implementation |
| --- | --- | --- |
| `trash()` | `mc_trash` | `NSFileManager` `trashItemAtURL:resultingItemURL:error:`; returns the destination selected by macOS |
| `choose_folder()` | `mc_pick_folder` | Modal `NSOpenPanel`, one directory, no files or directory creation; cancel returns an empty path |
| `app_info()` | `mc_app_info` | Reads `Contents/Info.plist`; extracts bundle identifier, nonempty display/name fallback, bundle name, and short version when values are strings |
| `app_name_for_bundle()` | `mc_app_for_bundle` | Looks up an app URL using `NSWorkspace`, rejects Trash/non-app paths, verifies its on-disk bundle ID, and returns its name; missing/stale/invalid results stay unrecognized |
| `running()` | `mc_is_running` | `NSRunningApplication` lookup by bundle identifier |
| `reveal()` | `mc_reveal` | `NSWorkspace` Finder selection request |
| `open_trash()` | `mc_open_trash` | `NSWorkspace` opens the home Trash URL |
| `open_privacy_settings()` | `mc_open_privacy_settings` | `NSWorkspace` opens one of two fixed System Settings URLs: Privacy & Security or Full Disk Access, selected by a boolean |

`c_path()` converts Rust paths to UTF-8 C strings and rejects unrepresentable paths or embedded NUL bytes. Native functions returning objects encode JSON through the local `json()` helper into allocated C strings. Rust's unsafe `response()` copies the bytes, frees the native allocation, parses JSON, and turns an `error` member into `Err(String)`. Other bridge functions return integers. Each native function uses an autorelease pool, and the bridge is built with ARC.

Finder reveal reports that the request was issued; it does not verify that Finder visibly selected the item. App-running checks are bundle based and do not detect every helper, command-line writer, package installation, or development server.

Cache lookup uses Apple's thread-safe [`URLForApplicationWithBundleIdentifier:`](https://developer.apple.com/documentation/appkit/nsworkspace/urlforapplication%28withbundleidentifier%3A%29); it does not launch apps. The ShipIt suffix follows [Squirrel's job identifier](https://github.com/Squirrel/Squirrel.Mac/blob/main/Squirrel/SQRLShipItLauncher.m) and [cache directory construction](https://github.com/Squirrel/Squirrel.Mac/blob/main/Squirrel/SQRLDirectoryManager.m).

Privacy settings use `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension`, with `?Privacy_AllFiles` for Full Disk Access. The frontend cannot pass an arbitrary URL. No permission is granted or current authorization inferred by this call. The macOS 13+ Full Disk Access link is described in [Apple's developer guidance](https://developer.apple.com/forums/thread/791996).

[`build.rs`](../src-tauri/build.rs) compiles the bridge with a macOS 13 deployment target, links Foundation/AppKit, and declares the command list for generated Tauri permissions. The implementation is macOS-specific, including its filesystem metadata and restore primitive.

## Tests

Scanner, cleanup, and native tests live next to their implementations behind `#[cfg(test)]`. Filesystem mutations use disposable fixtures; the native application lookup test also reads Finder's registered metadata. The native Trash round trip and the real-home read-only timing scan are opt-in ignored tests. Exact commands, fixture behavior, and coverage gaps are in [Development and testing](development.md#testing).
