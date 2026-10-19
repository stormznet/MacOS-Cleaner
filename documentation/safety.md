# Safety, limitations, and troubleshooting

[Documentation index](README.md) · [Implementation details](backend.md)

Mac Cleaner moves selected candidates to Trash and records their outcomes. Restoration depends on those items remaining accessible. It cannot recover data after Trash is emptied, replace a backup, or guarantee that removing every recognized cache is consequence-free.

## Safeguards and their boundaries

| Safeguard | What it enforces | Boundary |
| --- | --- | --- |
| Explicit selection and review | No automatic selections; exact IDs captured for review; acknowledgement before UI confirmation | The acknowledgement is a UI rule, not proof that all writers have stopped |
| Rust item registry | Cleanup resolves known scan IDs; the UI cannot supply an arbitrary cleanup path | IDs are valid for the current registry, not permanent references |
| Eligibility and scope | Complete/selectable candidate strictly under its stored scan root | Displayed totals can include items that cannot be removed |
| Ancestry and identity checks | Absolute paths without `..`, no symlink ancestry at validation, full top-level identity before move | Checks and the later native operation are not one atomic path-resolution transaction |
| Same-device checks | Cleanup requires the home's device; measurement rejects crossing devices | External-volume cleanup/restore is unsupported |
| Associated-file restrictions | Exact bundle IDs or unique app-name matches in fixed Library locations; app required except for independent user caches; failed app keeps associated data | Name matching does not establish all contents are disposable or exclusively owned; app selection includes eligible associated data by default |
| Running-app checks | Blocks operations for a known bundle reported running by macOS | Command-line tools, helpers, build servers, and new processes can escape this check |
| Native Trash | Uses macOS Trash and verifies the returned identity | A native error or unverified result requires inspecting history/Trash; no permanent-delete fallback |
| Durable history intent | Saves before a move/restore, saves outcome afterward, reconciles interrupted state | The file move and history write are separate operations; whole batches are not atomic |
| Exclusive restore | Destination must be absent; `RENAME_EXCL` prevents overwrite races | Missing parents, inaccessible sources, or parent-directory replacement can still prevent or complicate restoration |
| Local locking and file modes | One app instance per state directory; private data directory and state-file creation modes | Does not protect against every process running as the same user or compromised local state |

Stop active writers before cleanup. Rechecking a directory's top-level modification time does not detect every write to a file deeper inside it. A process can also change a path after validation. This implementation does not provide a filesystem snapshot or a complete defense against adversarial concurrent filesystem changes.

## Size and timing interpretation

Sizes sum allocated filesystem blocks (`st_blocks × 512`), deduplicating hard links within each candidate. They are estimates of storage on disk, not file-content lengths. Hard links shared across candidates, APFS clones/shared blocks, snapshots, compression, and later changes can make displayed totals differ from eventually reclaimed space.

Moving a candidate to Trash does not itself reclaim its blocks. Even after Finder empties Trash, the displayed estimate is not a guarantee of the free-space increase.

The scan has no ten-second deadline. It discovers all supported categories, including application inventories, before parallel measurement. The two recorded default-location smoke scans took 34.814 seconds initially and 10.363 seconds on a repeat; neither included configured project roots. These are environment-specific observations, not a benchmark guarantee. See [VALIDATION.md](../VALIDATION.md) for the complete context.

## Permissions, privacy, and persistence

The app runs with the current user's access. It has no administrator helper or automatic privilege escalation. Some protected locations can be inaccessible; errors or inspection-only/incomplete rows explain many exclusions. Certain rejected app candidates are skipped without a notice, so the scan is not an exhaustive inventory.

Settings includes a request/check action and shortcuts to Files & Folders and optional Full Disk Access. The check lists only fixed user folders and saved project roots, without reading file contents or changing files; macOS may prompt for folder consent. Results describe actual top-level directory access, not global authorization or write access. Missing folders are distinct from failed access. The native bridge opens fixed system URLs and reveals the running app bundle for manual Full Disk Access setup. It never enables permissions or queries private permission databases. Checks refresh when the app regains focus after opening System Settings. Grant access yourself in macOS, reopen the app if requested, and rescan. Choosing project roots remains necessary regardless of permission settings.

Settings and history live in `~/Library/Application Support/com.maccleaner.desktop/`. History includes original paths, Trash paths, names, sizes, timestamps, and file identities. It is local plaintext JSON with filesystem permissions, not encrypted storage. There is no account, upload, telemetry, history export, history-clear UI, or automatic history pruning. Scan results are held only in memory.

The main webview is restricted to the declared Tauri commands and window dragging. The configured CSP permits local scripts, local/data images, IPC connections, and inline styles used for chart values; it excludes objects and frames and restricts the base URI. No filesystem or shell plugin is exposed. These boundaries reduce the webview's authority; backend validation remains necessary.

The production app contains no remote-service integration. Development mode separately starts a loopback Vite server. Tests that mock desktop IPC are limited to test pages.

## Uninstall scope

Ordinary app bundles from the supported application roots and recognized associated paths in the user and system Libraries are candidates. Selecting an app includes eligible detected associated data by default. Review each path and uncheck local documents or other data to retain before confirming Trash. Apple app bundles are inspection-only, and Mac Cleaner excludes its own bundle.

The app does not discover/remove all vendor components, shared support directories, group containers, extensions, privileged helper binaries, service registrations, custom data paths, or drivers. App containers are recognized only by exact bundle ID. Launch configuration files are not an instruction to unload a service. It does not prove that an app is safe to uninstall by moving its bundle. Use the app vendor's procedure when required.

## Troubleshooting

| Symptom or message | Meaning and next step |
| --- | --- |
| Browser Scan says filesystem access requires desktop | Run `npm run desktop` or open the built `.app`; a browser preview intentionally cannot scan |
| node_modules shows Not scanned | No project folder is configured. Use Add folder & scan on node_modules; Scan also prompts for a folder when none is configured |
| No node_modules results inside a configured root | Confirm the chosen folder contains the project and check scan notices/Settings → Permissions; candidates need an immediate parent `package.json`; hidden/bundle/symlink/other-device subtrees are skipped |
| Project folder is rejected | Choose a real directory within home, outside Library, Applications, Trash, `.git`, and dependency directories; other devices are unsupported |
| Unknown owner / disabled checkbox | The item is inspection-only; inspect the path in Finder rather than assuming it is safe to remove |
| Calculating or Incomplete | Wait for scan completion, or rescan after cancellation/access problems; incomplete candidates cannot be cleaned |
| Item changed since scanning | Stop writers and run a fresh scan; do not reuse the previous selection |
| Quit the owning app | Quit the identified app before moving/restoring its data; also stop related tools or builds manually |
| Another operation is still running | Let the active operation finish; Stop scan applies only to scanning |
| Cleanup returns an error after some moves | Check every history row and Trash before retrying; some earlier items may already have moved |
| Needs inspection | The native move outcome could not be verified; use Finder and allow startup reconciliation to inspect recorded identity |
| Restore destination already exists | Mac Cleaner keeps both current destination and Trash item intact; decide in Finder how to preserve/relocate the conflict before retrying |
| Original parent is unavailable | The app will not recreate it; restore the intended parent structure or recover the item manually through Finder |
| Unavailable history item | It may have been moved, removed from Trash, or become inaccessible; inspect Finder. The app cannot recover emptied Trash |
| History does not reflect a manual Finder move | Reconciliation runs at startup/before restore and has limited identity-based rules; it is not live Finder monitoring |
| Already running | Use or close the existing app instance; the lock is held by the process, so deleting a lock filename is not a proper fix |
| Cannot read cleanup history | Startup preserves the unreadable state file. Keep a backup before any manual repair; there is no built-in repair/migration UI |
| History write failed | The operation stops further moves. Preserve the state files and inspect History/Trash before attempting another cleanup |
| Development/test port is busy | Stop the process you own using port 1420 or 1427, then rerun the relevant script; servers use strict ports |
| Native bridge rejects a path | The bridge requires a UTF-8 path without embedded NUL; inspect unsupported paths in Finder |

Restore operates on one history row at a time. Restoring an app does not automatically restore its associated data. Reinstalling an app or dependencies can create a destination conflict, which the app will refuse to overwrite.

## Release and validation limits

The prepared package is a local Apple Silicon build with an ad-hoc signature. Developer ID signing, notarization, and Gatekeeper distribution checks remain undone. macOS 13 is configured as the minimum, but older macOS and Intel hardware have not been tested.

Automated browser tests cover interface behavior with synthetic IPC, and Rust tests cover focused filesystem safeguards plus a real Trash/restore fixture. Native UI automation was denied access to the app during validation; live native window controls, folder selection, Finder reveal, and VoiceOver remain unverified. Representative large node_modules/Xcode benchmarks and adversarial concurrency tests also remain outstanding. The dated [validation record](../VALIDATION.md) is the source for observed results.
