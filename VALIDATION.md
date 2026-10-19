# Validation, 19 October 2026

Environment: Apple Silicon, macOS 27.0.1, Rust 1.99.1, Node.js 22.23.2, Tauri 2.12.4. macOS 13 is the configured deployment minimum; older macOS and Intel hardware have not been tested.

## Backend and build

- TypeScript check and Vite production build: passed.
- Rust formatting and Clippy with warnings denied: passed.
- Five focused Rust tests: passed, including the opt-in native macOS Trash test.
- Application bundle: builds for arm64. Launch confirmed by the running process and successful frontend-to-Rust bootstrap saving empty settings/history.
- Native UI automation: the Computer Use tool denied access to Mac Cleaner. Native window controls, the folder picker, Finder reveal, screen-reader use, and a complete interactive cleanup in the native window have not been verified. Backend Trash behavior was tested directly on disposable fixtures.
- Local package signing: ad-hoc signed with the application identifier and verified with `codesign --verify --deep --strict`. The ZIP passed archive integrity verification. Developer ID signing and notarization were not performed; this is a local development build.

The backend tests verify dependency discovery without following symlinks, preservation of manifests/lockfiles and unselected files, scan cancellation during measurement, stale item rejection, overlap rejection, invalid bundle identifiers and parent-path traversal rejection, interrupted move reconciliation, restoration without overwriting conflicts, and a real native Trash/restore round trip.

## UI interactions

Two browser test scenarios cover the following using an honest empty preview and explicitly synthetic IPC fixtures. Fixture screenshots are test evidence, not measurements of the user's Mac.

| Interaction | Observed behavior |
| --- | --- |
| Browser Scan | Explains that filesystem access requires the desktop app |
| Category and tool navigation | Opens the corresponding real view |
| Appearance | System, light, and dark modes update correctly |
| Narrow viewport | Overview, dependency view, history, and settings have no document overflow at 390px |
| Initial and streamed scan results | No items are preselected |
| Keyboard checkbox selection | Space selects the item and focus is retained |
| Search and sort | Filter/sort results while retaining explicit selection |
| Select visible / Clear | Changes only the intended visible selection; clear returns to zero |
| Item detail / reveal | Displays path and consequences; sends the item's ID to the backend |
| Review dialog | Shows exact chosen items and consequences; Escape cancels without a mutation |
| Confirmation | Requires the affected-apps/builds acknowledgement before the Trash action |
| Cleanup | Sends only selected item IDs and shows per-item history |
| Restoration | Changes the history status to Restored after backend success |
| App-associated files | Cannot be selected without the app; deselecting the app clears its associated data |
| Uninstall failure | Shows Not moved and the error, without offering restoration of an unmoved app |
| Unknown cache owner | Checkbox is disabled |
| HTML-like filenames | Render as text, without creating executable elements |
| Project roots | Add and remove update the configured roots |

Chromium suite: passed (2 scenarios). WebKit 27.2 suite: passed (2 scenarios). Browser page-error collections are empty on passing runs. Keyboard selection was checked using focus plus Space because WebKit does not necessarily focus a checkbox on a pointer click. Native IPC and macOS authorization behavior are not simulated by these browser tests.

## Scan timing

Two read-only smoke scans used the current machine's default locations with **no project roots configured**, through the Rust test build. They printed only aggregates and did not move any user files:

| Run | Candidates | Duration | Notices |
| --- | ---: | ---: | ---: |
| Initial run | 366 | 34.814 seconds | 8 |
| Repeat run, metadata already cached | 364 | 10.363 seconds | 8 |

The live filesystem changed slightly between runs. In the repeat run, 265 application-related items completed by 10.363 seconds and 91 cache items completed by 8.933 seconds. Other candidates were incomplete or inaccessible and remained ineligible for cleanup.

These are individual smoke measurements, not a controlled corpus benchmark or p95 guarantee. In particular, node_modules and Xcode performance have not been benchmarked on representative large project trees. The interface reports actual elapsed time and partial results. The ten-second target is close on the repeat run and not met on the initial run.

## Remaining release checks

- Representative cold/warm project benchmarks, including many small files and large dependency trees.
- Native folder selection, Finder reveal, live window interaction, and VoiceOver.
- Older supported macOS versions and Intel hardware if Intel distribution is needed.
- Developer ID signing, notarization, and Gatekeeper distribution validation.
- Adversarial concurrent filesystem changes: path/identity revalidation and exclusive restoration are present, but directory contents are not snapshotted. Quit active writers before cleanup.

No personal files were moved or removed during validation. Destructive tests operated only on temporary test-owned fixtures.

## node_modules setup and permissions fix, 19 October 2026

The reported missing node_modules results were traced to an empty saved project-root list. The scanner previously skipped project discovery without a notice, and the first Scan did not ask for a project folder. The fix prompts before scanning when no roots are configured, automatically scans after Add folder & scan on the dependency page, and labels the category Not scanned if the folder picker is cancelled. The backend also reports the skipped category in scan notices. Existing settings and cleanup history were not edited to work around the issue.

Settings now includes Files & Folders guidance, a Privacy & Security button, and an optional Full Disk Access button. A narrowly allowed native command opens fixed System Settings URLs. It does not grant or claim to detect permissions.

- TypeScript check and frontend production build: passed.
- Rust default tests: 4 passed; native Trash integration and real-home timing tests remain opt-in and were not rerun for this change.
- Rust formatting and Clippy with warnings denied: passed.
- Chromium and WebKit: 3 scenarios passed in each engine. New regression coverage checks first-scan folder selection, cancellation, picker-error recovery, immediate scan after adding a folder, root reuse on subsequent scans, correct permission-command arguments, and no cleanup calls during permission setup.
- Documentation: 129 local links/anchors and all 11 commands checked against documentation, build manifest, capabilities, and generated permissions.
- The new Settings layout was inspected using a synthetic browser screenshot. Native folder-picker interaction and System Settings navigation remain unverified; browser mocks do not test macOS authorization.
- Updated Apple Silicon application bundle: built successfully (6.13 MiB). The local ad-hoc signature passed strict verification, and the refreshed `release/MacCleaner-0.1.0-arm64.zip` passed archive integrity verification.

No user files were moved and no macOS permissions were changed by this fix or its tests.

## Scan percentage and scrolled details fix, 19 October 2026

A browser regression test reproduced the reported detail issue before the fix: the selected lower row's detail heading had zero viewport intersection. Details now stay beside desktop results with a sticky, independently scrollable pane, scroll into view on narrow layouts, preserve their reading position across renders, and restore focus/visibility to the selected row on close.

Scanning now emits processed/total events after discovery establishes the candidate count. The UI shows indeterminate discovery, then the percentage of checked candidates; failed measurements count as checked. It reserves 100% for a completed command, preserves partial progress on cancellation, and handles empty scans without division errors. Progress is available under every page toolbar.

- TypeScript check and production frontend build: passed.
- Rust default tests: 5 passed, 2 opt-in tests skipped. The new fixture test verifies progress for successful and failed checks, cancellation, and serialization without scanning user locations.
- Clippy with warnings denied: passed.
- Chromium and WebKit: all 5 scenarios passed in each engine. New checks cover progress lifecycle/ordering and details at 1180, 900, and 390 px, including inner scrolling and focus restoration.
- Synthetic screenshots of the percentage bar and scrolled details were visually inspected. These do not replace an interactive native-webview check.
- Apple Silicon bundle rebuilt successfully (6.13 MiB); the refreshed local ad-hoc signature passed strict verification and the ZIP passed archive integrity verification. Documentation links and the new progress contract were checked.

The real-home timing scan and native Trash integration test were not rerun. No user files were moved or permissions changed.

## App-cache owner recognition fix, 19 October 2026

The scanner previously matched cache names only to bundle IDs discovered under `/Applications` and `~/Applications`. This missed app-name folders, ShipIt updater caches, and registered applications elsewhere. It now matches unique app names and bundle filenames (ASCII case-insensitive), resolves ShipIt caches to the parent app, and uses macOS's registered-application lookup with an on-disk bundle-ID check as a fallback. Ambiguous names, arbitrary prefixes, unconfirmed owners, and Mac Cleaner's own cache remain ineligible. Cache descriptions identify recognized owners; ownership failures now explain why review is needed.

- Rust default tests: 7 passed, 2 opt-in tests skipped. New regression coverage checks ID/name/updater matches, ambiguity, rejected prefix guesses, native Finder lookup, missing/invalid identifiers, and an empty display-name plist fallback.
- TypeScript check, production frontend build, Rust formatting, and Clippy with warnings denied: passed.
- Chromium and WebKit: all 5 existing UI scenarios passed in each engine.
- Explicit read-only scan: 368 candidates in 7,412 ms, with 9 notices. Among app caches, 25 owners were recognized and 77 were unresolved or excluded; 94 cache entries completed measurement. No project roots were configured for this smoke test, and cleanup history remained empty.
- Documentation: 129 local links and anchors passed validation; ownership behavior, native functions, and tests are documented.
- Updated Apple Silicon application bundle: built successfully (6.13 MiB). The refreshed local ad-hoc signature passed strict verification, and `release/MacCleaner-0.1.0-arm64.zip` passed archive integrity verification.

The native Trash test and interactive native-window checks were not rerun. No user files were moved or permissions changed.

## Renamed node_modules cleanup fix, 19 October 2026

Discovery previously required the exact filename `node_modules`, missing copies such as `node_modules 10-58-12-759`. A shared name predicate now accepts the canonical name, ASCII-numbered copies, and valid `HH-MM-SS-mmm` suffixes. Discovery and project-root exclusions use the same predicate. The existing parent `package.json`, symlink, volume, cloud-placeholder, and no-nested-candidate rules remain in effect. Renamed rows include the actual folder name and explain that only the selected copy is moved.

Restore already uses the recorded original path instead of the basename assigned in Trash. Its regression fixture now restores a timestamp-renamed directory to `node_modules`, verifies its contents, and rejects an existing destination without overwriting either copy.

- Rust default tests: 8 passed, 2 opt-in tests skipped. The new discovery test checks supported suffixes, malformed/unrelated names, nested/orphan/hidden/symlink exclusions, root validation, and retained project files. It also passed independently after simplifying its expected-path assertion.
- Explicit native Trash integration: passed. It discovered, trashed, and restored only an owned timestamped test folder; the unselected canonical dependency directory, manifest, and lockfile remained intact.
- TypeScript check, frontend production build, Rust formatting, and Clippy with warnings denied: passed.
- Chromium: all 5 UI scenarios passed. WebKit: 4 passed initially; the existing scrolled-details scenario timed out waiting for its heading, then passed in isolation at all 3 viewport widths. Frontend source was unchanged in this fix; the intermittent UI check is not treated as a clean full-suite pass.
- Documentation: 129 local links and anchors passed validation; supported names, selection behavior, restore names, root exclusions, and test coverage are documented.
- Apple Silicon application bundle rebuilt successfully (6.13 MiB). The local ad-hoc signature passed strict verification, and the refreshed `release/MacCleaner-0.1.0-arm64.zip` passed archive integrity verification.

Native window interaction and the real-home timing scan were not rerun. File moves were limited to the integration test's own temporary fixture; no user project folders were cleaned.


## DMG releases and update notification, 19 October 2026

- DMG-only release build passed with ad-hoc signing outside the sandbox. `hdiutil verify` passed; the mounted app passed `codesign --verify --deep --strict`, and its Applications shortcut points to `/Applications`. The image was detached afterward. Distribution copy: `release/MacCleaner-arm64.dmg` (2.11 MiB).
- Workflow YAML and every shell run block passed syntax checks. CI verifies the app inside the mounted DMG because Tauri removes its temporary staging app for DMG-only builds.
- The GitHub repository API returned HTTP 200 and confirms the repository is public, as required by the unauthenticated update check.

- Frontend production build and typecheck passed.
- Chromium: all seven UI scenarios passed, including the two new update scenarios.
- WebKit: all seven UI scenarios passed outside the sandbox, which blocks the test server's listening socket.
- Rust: eight tests passed, two opt-in filesystem/scan tests intentionally skipped. The existing Finder-registration test requires running outside the sandbox.
- Rust formatting and Clippy with warnings denied passed.
- Update fixtures cover newer/equal/older versions, numeric ordering, drafts/prereleases, invalid versions, missing/null/untrusted DMG assets, 404, rate-limit errors, and invalid JSON. Browser tests block real GitHub requests.
- Mocked desktop tests verify keyboard download activation, the exact `open_update` command, manual retry, native-launch error feedback, and continued scan availability after update-check failure.
- No GitHub release was published. Actual GitHub delivery and the default-browser launch remain unverified; no existing app was replaced. Developer ID signing and notarization remain unconfigured.

Antislop delivery gate, scoped to the new update controls:

- Hard Gate PASS: new copy/assets inspected; no invented data/assets or dead controls; Chromium and WebKit tests found no horizontal overflow at 1180, 900, or 390 px in light/dark themes. Preview is explicitly disabled with an explanation; check failures provide a retry action.
- Purpose Gate PASS: existing secondary buttons and settings rows provide the notification and retry actions; no new decoration, color, animation, or navigation pattern was introduced.
- Liveliness PASS: retained DESIGN.md's warm macOS utility direction and energy 2/rhythm 2/motion 1; the update action shares the existing toolbar and leaves scan content as the focal point.
- C-1 PASS: controls exist to download a discovered DMG or retry a release check.
- C-2 PASS: mocked keyboard activation invokes `open_update`; native command accepts no arbitrary URL and uses the fixed repository DMG address.
- C-3 PASS: application version, check status, and retry share one settings section; no unrelated sections were added.
- C-4 PASS: both browser suites cover desktop/narrow widths and light/dark appearance; keyboard Enter works; update text contrast is 10.44:1 dark and 12.57:1 light using the skill's contrast checker.
- C-5 PASS: installed version comes from Tauri configuration; newer-release status requires GitHub metadata and the expected DMG asset. Test screenshots contain explicit synthetic fixtures.
- Consistency locks PASS: existing spacing, type, radius, layout, theme tokens, state treatment, and button styling are reused; no CSS or visual assets changed. `test-results/update-available.png` was visually inspected.
