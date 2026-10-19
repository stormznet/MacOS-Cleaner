# Development and testing

[Documentation index](README.md) · [Source map](architecture.md#complete-source-and-configuration-map)

Run commands below from the project root. They describe the scripts and test harness included in this repository.

## Requirements and setup

The application is macOS-specific. The configured deployment minimum is macOS 13; the validated local build is Apple Silicon on macOS 27.0.1. It needs Xcode/Command Line Tools, Node.js 22.12+ or a compatible newer release, npm, and stable Rust with Cargo. The recorded environment used Node 22.23.2 and Rust 1.99.1; older supported macOS and Intel have not been validated.

```sh
npm ci
npm run desktop
```

`npm ci` uses the committed npm lockfile. The `tauri` npm script adds `~/.cargo/bin` to its process PATH. `desktop` starts Tauri development mode, whose configured `beforeDevCommand` starts Vite.

For an interface-only preview:

```sh
npm run dev
```

Open `http://127.0.0.1:1420`. Preview data starts empty and native operations are unavailable. Do not interpret browser preview as a functional filesystem cleaner.

## Every npm script

| Script | Current command | Purpose |
| --- | --- | --- |
| `dev` | `vite --host 127.0.0.1` | Local frontend server, port 1420 |
| `build` | `tsc --noEmit && vite build` | Strict typecheck and frontend production build |
| `tauri` | `PATH="$HOME/.cargo/bin:$PATH" tauri` | Tauri CLI with the conventional Rust installation directory in PATH |
| `desktop` | `npm run tauri -- dev` | Native development app with Vite |
| `bundle` | `npm run tauri -- build --bundles dmg` | Release `.dmg` installer, including frontend build |
| `test` | `node --test tests/ui.test.mjs` | Chromium UI suite |
| `test:webkit` | `UI_ENGINE=webkit node --test tests/ui.test.mjs` | WebKit UI suite |

`vite.config.ts` requires its selected port to be free and excludes `src-tauri/` from frontend watch events. Production JavaScript targets Safari 16. `tsconfig.json` uses strict checking, ES2022, ESNext modules, bundler module resolution, and no TypeScript emission; Vite emits the distributable frontend.

## Native build configuration

[Cargo.toml](../src-tauri/Cargo.toml) defines the Rust 2021 binary, runtime/build/test dependencies, and a release profile with LTO, one codegen unit, and stripped symbols. The `custom-protocol` feature forwards to Tauri. [build.rs](../src-tauri/build.rs) compiles the Objective-C source with ARC and a macOS 13 deployment target, links AppKit/Foundation, and asks Tauri to generate configuration/command permissions.

[tauri.conf.json](../src-tauri/tauri.conf.json) defines:

- Product `Mac Cleaner`, identifier `com.maccleaner.desktop`, version `0.1.1`.
- Main window 1180 × 800, minimum 900 × 620, overlay title bar, hidden title text.
- Vite development URL and `../dist` production frontend.
- Main-window capability and content-security policy.
- `.dmg` bundle target, PNG/ICNS icons, macOS 13 minimum.

The SVG in `src-tauri/icons/` is the editable source; the PNG and ICNS are the configured bundle assets. If changing the app icon, regenerate its packaged forms as well. If changing the application identifier, account for the changed application-data directory and self-exclusion in `scan.rs`. There is currently no migration for an old identifier's history.

Release versions in `package.json`, `Cargo.toml`, `tauri.conf.json`, and corresponding lockfiles must agree. Vite injects the Tauri version into the UI and update comparison at build time. The published DMG keeps the fixed name `MacCleaner-arm64.dmg`, used by the update button.

## Testing

### Frontend build and browser tests

```sh
npm run build
npx playwright install chromium webkit
npm test
npm run test:webkit
```

The test file uses Node's built-in test runner with Playwright browser APIs/assertions. It starts its own Vite server on `127.0.0.1:1427`, launches the selected engine, creates `test-results/`, and closes browser/server afterward. A separate running desktop app or port-1420 server is unnecessary.

Seven scenarios in [tests/ui.test.mjs](../tests/ui.test.mjs) cover:

| Scenario | Main assertions |
| --- | --- |
| Update metadata | Newer/equal/older numeric versions, stable-only releases, missing or untrusted DMG assets, malformed responses, unavailable releases, and API errors |
| Update controls | Startup detection, keyboard download action, manual retry, download failures, enabled scan controls, dark/light layouts at 1180/900/390 px |
| Scan progress | Indeterminate discovery, 0/25/50/99/100% updates, monotonic handling of reordered counts, visibility across page navigation, completion with notices, empty scan, cancellation, and command failure |
| Scrolled details | Lower-row heading/controls visible at 1180/900/390 px, sticky desktop behavior, preserved inner scroll across rerenders, access to the end of long details, no page overflow, focus/visibility restored to the original row on close |
| Browser preview | Empty state, disabled empty selection, native-access explanation, category/tool navigation, three appearance modes, no overflow at 390 px on selected screens, no page errors |
| First-scan scope and permissions | Empty settings prompt before scan; chosen folder yields unselected dependency results; cancelled picker shows Not scanned; picker error releases controls and can be retried; Add folder & scan starts scanning; repeat scans reuse roots; two permission buttons send the correct boolean without cleanup calls |
| Synthetic desktop IPC | No auto-selection, keyboard focus/Space, search and sort, visible-selection/Clear, details/Finder action, review acknowledgement and Escape, exact submitted IDs, returned history and restore, app/child selection constraints, failed-uninstall display, unknown-cache ineligibility, HTML-like filename escaping, root add/remove, no page errors |

The desktop scenarios inject fake `__TAURI_INTERNALS__` only into the test page. Their data paths are synthetic. Fixtures invoke the channel callback directly and supply command responses; they do not exercise the real Rust handlers, command ACL, native IPC transport, macOS authorization, System Settings navigation, or native Finder behavior.

Screenshots include `overview-dark.png`, `settings-light.png`, `overview-fixtures.png`, `results-fixtures.png`, `permissions-fixtures.png`, `scan-progress-fixtures.png`, and `details-scrolled-1180.png`/`details-scrolled-900.png`/`details-scrolled-390.png`. The fixtures and scrolled-details screenshots contain synthetic test data, not measured user data. Both browser engines write the same filenames, so a later run replaces earlier screenshots.

### Rust checks

```sh
~/.cargo/bin/cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml
~/.cargo/bin/cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

These checks compile native macOS code. Running the default suite executes eight tests and skips two opt-in tests. Cache regression coverage checks exact IDs, ShipIt ownership, name aliases, case differences, ambiguous names, and rejection of guessed prefixes. The native lookup test reads the registered Finder app, checks missing/invalid identifiers, and uses a temporary plist for the empty-display-name fallback:

| Test | Coverage |
| --- | --- |
| `cache_owners_match_names_and_updaters_without_guessing_shared_owners` | Bundle IDs, ShipIt parent IDs, name aliases, ASCII case, ambiguous names, and rejection of arbitrary prefixes/suffixes |
| `resolves_registered_apps_and_handles_missing_metadata` | Native registered-app lookup, missing/NUL-containing identifiers, empty-display-name fallback to bundle name |
| `progress_counts_failed_checks_and_stops_advancing_after_cancellation` | Fixed total, progress for both readable/missing candidates, incomplete-item safety, cancellation freezes counts, serialized progress event shape |
| `discovers_only_project_dependencies_and_never_follows_symlinks` | One outer dependency candidate, orphan notice, hidden/symlink exclusion, allocated size, measurement cancellation |
| `discovers_renamed_dependencies_without_nested_or_unrelated_candidates` | Timestamped/numbered copies, invalid suffix rejection, no nested/orphan/symlink/hidden candidates, root exclusions, preserved lockfile |
| `rejects_bundle_identifiers_that_could_escape_related_data_paths` | Unsafe bundle-ID forms, accepted valid form, relative and parent-traversal path rejection |
| `rejects_changed_items_symlinks_and_overlapping_selections` | Replacement identity, symlink ancestry, overlap rejection, retained fixture content |
| `restores_without_overwrite_and_reconciles_interrupted_moves` | Locate a timestamp-renamed directory in test Trash by identity, preserve destination conflict, restore the original `node_modules` name and bytes, recognize interrupted restore |
| `native_trash_roundtrip_keeps_unselected_files` — ignored | Discover and Trash/restore an owned timestamped dependency copy; the unselected active dependency folder, parent manifest, and lockfile remain |
| `local_read_only_scan` — ignored | Current-Mac scan against real default locations, aggregate timing/candidate/category/cache-ownership output, empty cleanup history |

Run the native integration test explicitly when verifying file operations:

```sh
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml native_trash_roundtrip_keeps_unselected_files -- --ignored
```

It uses the real current user's Trash for a test-owned temporary fixture and restores that fixture. Unlike browser tests, it performs an actual filesystem move. If interrupted, inspect the fixture's result rather than assuming its automatic cleanup finished.

The optional timing scan reads real default scan locations but configures no project roots and stores its app state in a temporary directory:

```sh
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml local_read_only_scan -- --ignored --nocapture
```

Do not use a blanket `--include-ignored` unless both opt-in behaviors are intended. A single live scan is a smoke measurement, not a representative benchmark or proof of a ten-second guarantee.

### Validation record

[VALIDATION.md](../VALIDATION.md) records the actual earlier build/test results and remaining checks. Native UI automation was denied access to the app, so native folder selection, window interactions, Finder reveal, and VoiceOver were not verified interactively. Browser success is not a substitute for these checks. Update that record with the environment and results when new validation is performed.

## Build and local packaging

```sh
npm run bundle
```

Output: a `.dmg` under `src-tauri/target/release/bundle/dmg/`, containing the `.app` and an Applications shortcut. Tauri removes the temporary `.app` staging folder for DMG-only builds. No ZIP packaging step is used.

For an ad-hoc signed local build without Finder layout automation:

```sh
APPLE_SIGNING_IDENTITY='-' npm run bundle -- --ci
dmg='src-tauri/target/release/bundle/dmg/Mac Cleaner_0.1.1_aarch64.dmg'
hdiutil verify "$dmg"
dmg_mount=$(mktemp -d)
hdiutil attach "$dmg" -readonly -nobrowse -mountpoint "$dmg_mount"
codesign --verify --deep --strict --verbose=2 "$dmg_mount/Mac Cleaner.app"
hdiutil detach "$dmg_mount"
```

The filename above assumes Apple Silicon and version 0.1.1. Developer ID signing, notarization, and Gatekeeper distribution validation have not been completed.

The [GitHub workflow](../.github/workflows/build.yml) builds for `aarch64-apple-darwin`, verifies the disk image and the signature of the app mounted from it, and copies the DMG to `release/MacCleaner-arm64.dmg`. Branch pushes, pull requests, and manual runs upload it as a workflow artifact; pushing a matching `v*` tag also publishes it as a GitHub Release. GitHub wraps workflow artifact downloads in its own archive; the release asset itself is a DMG. The workflow does not run the test suites listed above.

The desktop update check fetches the latest stable release from the public GitHub API. It requires a strictly newer version and the expected DMG asset/URL before showing the update button. The native `open_update` command opens only a fixed HTTPS URL for that repository's latest DMG, without accepting a URL from the frontend. Users install it manually; no updater signing keys, self-installation, or restart flow are used. Keep this URL, the frontend repository URL, and the workflow's DMG filename aligned if moving the repository. See the [update instructions](../README.md#application-updates).

## Where to make changes

| Change | Places to update and checks to consider |
| --- | --- |
| Add a scan location | `scan.rs` discovery, explicit scope/description/consequence/ownership; scanner fixture; features guide |
| Add a category | Scanner category, `types.ts` metadata/types, `main.ts` navigation/overview key lists, `tokens.css`, icons if needed; UI test and docs |
| Change selection or review | `main.ts`, backend constraints in `cleanup.rs` if semantics change, mocked IPC fixture and browser assertions |
| Change Trash/restore | `cleanup.rs`, model/persistence as needed, native bridge if needed; safety tests and opt-in native fixture; backend/API/safety docs |
| Add a command | Handler and registration in `main.rs`, manifest in `build.rs`, main capability, frontend caller/types, relevant tests; API table |
| Change saved data | `model.rs`, frontend consumed types, load compatibility/migration decisions; history integrity and recovery checks |
| Change appearance/layout | `tokens.css`, `style.css`, relevant template; both browser engines, keyboard focus, narrow widths |
| Change native bridge | Matching C/Rust signatures, memory ownership, `build.rs` if linking changes; targeted native test/manual check |

Generated command permission files say not to edit them manually. Change the command manifest and rebuild to regenerate them; explicitly allow the command for the intended window. Never add broad filesystem/shell access merely to bypass an application command's validation.

When maintaining these docs, check relative links, field names, command names, and source-file coverage. Documentation-only changes do not require a fresh cleanup operation or real-home scan.
