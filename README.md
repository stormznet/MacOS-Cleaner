# Mac Cleaner

A macOS desktop app built with Rust, Tauri 2, and TypeScript. The interface follows `example-image.png`: a category sidebar, storage breakdown, review lists, item details, and a persistent selection bar.

See [documentation/README.md](documentation/README.md) for the complete feature guide, architecture and source map, Rust/native and frontend walkthroughs, IPC/data reference, development/testing instructions, and safety troubleshooting.

## Run

Requires macOS 13+, Xcode Command Line Tools, Node.js 22.12+ or a compatible newer release, and stable Rust. The current build targets Apple Silicon.

```sh
npm ci
npm run desktop
```

The npm desktop scripts include `~/.cargo/bin` in their PATH. Install Rust from [rustup.rs](https://rustup.rs) if it is not already installed.

```sh
npm run bundle
```

The application is written to `src-tauri/target/release/bundle/macos/Mac Cleaner.app`. The prepared package is `release/MacCleaner-0.1.0-arm64.zip`, with an ad-hoc signature verified for local use. This is a local development build, without Developer ID signing or notarization. Public distribution requires an Apple signing identity and notarization.

`npm run dev` opens the frontend development server. A regular browser shows an empty preview and cannot scan, uninstall, or modify files. Use the desktop app for filesystem operations.

## GitHub builds and releases

[Build and release](.github/workflows/build.yml) runs on branch pushes, pull requests, and manual dispatches. It builds the Apple Silicon app on macOS, verifies its ad-hoc signature, and uploads `MacCleaner-macos-arm64` under the workflow run's artifacts. The artifact contains `MacCleaner-<version>-arm64.zip`.

Pushing a `v*` tag also publishes a GitHub Release with the ZIP and generated release notes. The tag must match `src-tauri/tauri.conf.json` (currently `v0.1.0`); a mismatched tag fails before building. Tags containing a hyphen, such as `v0.2.0-beta.1`, are marked as prereleases. Before bumping a release, keep the versions in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, the lockfiles, and the UI version text in sync.

After committing the release version and pushing that commit to GitHub:

```sh
git tag -a v0.1.0 -m "Mac Cleaner v0.1.0"
git push origin v0.1.0
```

The release job uses the built-in `GITHUB_TOKEN`; no extra repository secrets are needed. These builds are ad-hoc signed and are not notarized. macOS may require approval in Privacy & Security before opening them.

## Use

1. Scan prompts for a project folder when none is configured. You can also add folders in Settings, or use **Add folder & scan** on node_modules. Cancelling the picker scans other categories and marks node_modules **Not scanned**.
2. Scan your Mac. The progress bar shows discovery first, then a percentage of items checked once the total is known. It reaches 100% when the scan finishes. Stop scan keeps the partial percentage and results.
3. Open a category. Inspect a row's path, explanation, and removal consequences, then select its checkbox. Details stay visible beside scrolled desktop lists and scroll into view on narrow layouts; long details scroll independently.
4. Review the exact selection, quit affected apps and stop builds, then move the selected items to Trash.
5. Use Cleanup history to restore an item. Existing files are never overwritten. Open Trash in Finder when you want to permanently remove its contents yourself.

Nothing is selected automatically. Moving files to Trash does not immediately free their disk space.

**Settings → Permissions** opens macOS Privacy & Security or Full Disk Access. Enable the needed folder access for Mac Cleaner yourself, then reopen the app and scan again. Full Disk Access is optional and does not replace choosing project folders.

## Included

- Xcode DerivedData and the recognized Xcode cache. Archives, simulators, runtimes, and source projects are excluded.
- npm, Yarn, pnpm, and Bun download-cache locations. Package-manager stores outside the explicit supported locations are excluded.
- node_modules and timestamped/numbered copies (such as `node_modules 10-58-12-759`) under user-selected project roots with a parent package.json. Each folder is a separate candidate, including its nested dependencies; parent manifests and lockfiles are kept.
- Per-app caches matched by bundle ID, unique app names, and ShipIt updater IDs, with a macOS application lookup fallback. Unconfirmed owners can be inspected but cannot be selected.
- Ordinary non-Apple app bundles in /Applications and ~/Applications, including subfolders up to two levels deep. Exact bundle-identifier preferences, support folders, and saved state appear as separate optional items. Shared vendor data and containers are excluded. Apps with privileged helpers, extensions, or vendor uninstallers need their vendor's removal workflow.
- Persistent cleanup history, native Trash movement, interruption reconciliation by file identity, and restoration with atomic no-overwrite behavior.
- Search, sorting, selection review, cancellation, notices for inaccessible paths, system/light/dark appearance, keyboard controls, and layouts down to narrow browser widths.

## Safety and boundaries

Rust retains scan items and accepts item IDs for cleanup. It rechecks scope, file identity, symlinks, device, and running app status before moving anything. Failed native Trash calls never fall back to permanent deletion. A single-process lock protects the local history. Pending operations are saved before changes; history writes use a temporary file, fsync, and atomic replacement.

Projects must be inside the current user's home folder, outside Library, Applications, and Trash. Other volumes and cloud placeholders are excluded. Close active writers before cleanup: revalidation detects replaced top-level items, but it does not provide a filesystem snapshot or prevent arbitrary concurrent writes inside a directory. Sizes are allocated-block estimates with hard-link deduplication inside each item; shared blocks across items and APFS clones can make totals differ from space eventually reclaimed.

History can restore only items that remain accessible in Trash. An unavailable original parent, a destination conflict, or missing Trash data is reported without overwriting files. When history is unreadable, the app fails startup rather than replacing it.

Settings and history are stored in `~/Library/Application Support/com.maccleaner.desktop/state.json`. There is no server, telemetry, account, privileged helper, or automatic Trash emptying.

## Checks

```sh
npm run build
npx playwright install chromium
npm test
npx playwright install webkit
npm run test:webkit
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml
~/.cargo/bin/cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The UI tests start their own local server on port 1427, use explicit synthetic fixtures for IPC, and write screenshots under `test-results/`. They never modify real files.

Opt-in native integration test, which creates and removes only its own temporary fixture:

```sh
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml native_trash_roundtrip_keeps_unselected_files -- --ignored
```

Optional read-only scan timing on the current Mac, with aggregate output only:

```sh
~/.cargo/bin/cargo test --manifest-path src-tauri/Cargo.toml local_read_only_scan -- --ignored --nocapture
```

See `VALIDATION.md` for the results and remaining validation limits. Ten seconds is a target, not a guaranteed duration for every project tree.
