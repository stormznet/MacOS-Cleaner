# Features and user guide

[Documentation index](README.md)

Mac Cleaner finds developer storage and installed applications, explains each candidate, and moves only explicitly selected items to macOS Trash. It does not empty Trash or schedule automatic cleanup.

## First cleanup

1. Start the desktop application. A normal browser is an interface preview with no filesystem access.
2. Open **Settings → Project folders → Add folder**, or use **Add folder & scan** on the node_modules page. Choose a folder containing your projects; adding it from node_modules immediately starts a scan.
3. Select **Scan my Mac** if needed. If no project folder is configured, Scan opens the folder picker first. Cancelling still scans other categories, with node_modules marked **Not scanned** and an explanatory notice. The same scan checks all supported categories, including installed apps.
4. Open a category and select an item's name to inspect its path, explanation, removal consequence, version where available, and modified date. **Show in Finder** opens its location.
5. Quit affected apps and stop development servers, builds, and package installations. Select individual checkboxes or use the visible-items checkbox.
6. Select **Review**, check the exact paths and consequences, acknowledge that affected processes are stopped, and choose **Move … to Trash**.
7. Check every result in **Cleanup history**. **Restore** returns an available item to its original location without replacing an existing file.

Nothing is selected automatically. Moving files to Trash is separate from permanently removing them. **Open Trash** opens Finder; emptying Trash is a separate user action outside Mac Cleaner.

## Overview and scan progress

The overview contains a storage total, a segmented category chart, four category cards, an application-manager link, and the current selection bar. The cleanup total includes node_modules, Xcode, package caches, and app caches. Application bundles and their associated data appear separately in the application manager.

The chart and category totals show discovered storage, including inspection-only items. They are not a promise that all displayed bytes can be removed. The selection bar counts only eligible, fully measured items that you selected.

During scanning, rows appear before their sizes are ready. They show **Calculating…**, and selection is disabled while the scan runs. A progress bar below the toolbar is available on every page. Discovery shows **Finding items…** with an indeterminate bar because the total is still unknown. Once discovery finishes, the percentage counts checked candidates out of the fixed total, including checks that end with an access error. This measures items checked, not bytes or time remaining. It stays below 100% until the scan command finishes, then shows **Scan complete** or **Scan complete with notices**. Notices explain many inaccessible or skipped locations; some invalid or unreadable app candidates are silently skipped, so the result is not a complete disk inventory.

**Stop scan** requests cooperative cancellation. It does not impose a fixed deadline or interrupt an operating-system call. Completed candidates remain available; unfinished candidates remain ineligible. The bar keeps the partial percentage and says **Scan stopped**, or reports that discovery stopped if no total was available. **Scan again** clears the previous progress, results, and selection and creates fresh item IDs. A completed empty scan shows 100% with zero items checked.

## Supported storage categories

Here, `~` means the current user's home folder. These locations are fixed in [scan.rs](../src-tauri/src/scan.rs); project roots are the only configurable scan locations.

| Category | Discovered locations | What removal means |
| --- | --- | --- |
| node_modules | `node_modules` and supported timestamped/numbered copies under chosen project roots, with a `package.json` file in the immediate parent | Reinstall active dependencies before using the project; local edits inside any selected dependency directory go with it |
| Xcode | Immediate children of `~/Library/Developer/Xcode/DerivedData`; `~/Library/Caches/com.apple.dt.Xcode` | Xcode may rebuild, reindex, or regenerate cached data |
| Package caches: npm | `~/.npm/_cacache` | Packages may need downloading again |
| Package caches: Yarn | `~/Library/Caches/Yarn` | Packages may need downloading again |
| Package caches: pnpm | `~/Library/Caches/pnpm` | Packages may need downloading again |
| Package caches: Bun | `~/.bun/install/cache` | Packages may need downloading again |
| App caches | Immediate children of `~/Library/Caches` | The owning app may regenerate data or start more slowly |
| Applications | `.app` bundles in `/Applications` and `~/Applications`, including two levels of containing subfolders | The selected app becomes unavailable until restored or reinstalled |

### node_modules rules

Each discovered dependency directory is one candidate, including everything inside it. Discovery stops at that directory instead of creating separate candidates for nested dependencies. The parent project's manifest and lockfiles are outside the removal candidate.

Names include `node_modules`, numbered copies such as `node_modules 2`, and timestamped copies such as `node_modules 10-58-12-759` (`HH-MM-SS-mmm`, 24-hour time). Other suffixes such as `node_modules notes` are not treated as dependency folders. Renamed candidates show both the project and actual folder name, with an explanation to review the copy. Selecting a copy moves only that folder; a sibling `node_modules` stays unless selected separately. These rules also exclude choosing a dependency copy or one of its descendants as a project root.

Mac Cleaner's History → Restore uses the recorded original path even if macOS assigned a different name in Trash. A folder already returned under a timestamped name is now discoverable on the next scan. Restore never replaces an existing original folder or silently creates a differently named copy.

Discovery skips hidden directories, `.app`, `.bundle`, and `.photoslibrary` directories, symbolic-link directories, other devices, and cloud placeholder directories. A dependency directory without a parent `package.json` is skipped with a notice. The manifest is checked for existence, not parsed to validate the project.

The whole home folder is not searched recursively by default. Choose a reasonably scoped projects folder; very large trees can take longer to discover and measure.

### Xcode and package-cache limits

Xcode archives, simulators, runtimes, device support, and source projects are outside this implementation's cleanup targets. Xcode must be closed before its recognized data can be moved.

Package-manager configuration is not inspected. Custom cache locations and stores outside the listed paths are not discovered. Stop package installations before cleanup. A location that also appears under app caches is included only once.

### App-cache ownership

A cache entry is recognized by an installed application's exact bundle identifier, a `<bundle-id>.ShipIt` updater folder, or a unique app display name, bundle name, or `.app` filename without its extension. Name matching ignores ASCII case; names shared by different bundle identifiers are not assigned to either app. Arbitrary vendor prefixes and helper suffixes are not guessed.

For bundle-ID folders missing from the app inventory, macOS's registered-application lookup can identify apps in other locations. The returned app's on-disk bundle identifier must match, and apps in Trash are rejected. This lookup only identifies cache ownership; it does not add those applications to the uninstaller. The recognized Xcode cache is also supported.

Recognized cache details include the app name and bundle identifier. ShipIt caches retain the parent app's running-app check and instruct you to wait for updates to finish. Mac Cleaner's own cache is excluded. Unconfirmed owners remain **inspection-only**: their sizes and paths are visible, but their checkboxes are disabled. The explanation distinguishes an unconfirmed owner from a permissions failure; a cache may belong to a removed app, a shared tool, or a macOS service.

Cache ownership is a filename match, not a guarantee about the contents. Review the description and path. App caches are independent candidates, so selecting an application does not automatically select its cache.

## Application uninstall

Open **Uninstall an app** after scanning. Select the app bundle first, then separately select any associated files you intend to remove. Selecting an app never automatically selects its associated data.

For a non-Apple app with a valid bundle identifier, the scanner checks these exact paths:

| Associated item | Path | Review consideration |
| --- | --- | --- |
| Preferences | `~/Library/Preferences/<bundle-id>.plist` | Removing this resets saved settings |
| Application support | `~/Library/Application Support/<bundle-id>` | May contain local user data |
| Saved application state | `~/Library/Saved Application State/<bundle-id>.savedState` | May contain saved window/session state |

Associated items require their parent app in the same selection. Deselecting the app clears its associated selections. If moving the app fails or cannot be verified, its selected associated data is kept. The backend enforces these rules independently of the UI.

Apple applications whose identifiers start with `com.apple.` are inspection-only. Mac Cleaner excludes its own bundle. Apps without a usable bundle identifier are skipped. `/System/Applications` is not scanned for uninstall candidates; macOS can still identify a system app as a user-cache owner.

This is removal of ordinary app bundles and narrowly matched optional files. It does not remove shared vendor folders, app containers, group containers, privileged helpers, login services, drivers, or extensions. Use a vendor's removal workflow when the app requires one. There is no automatic detection proving that an app has no such components.

## Search, sorting, and selection

- Search matches names and full paths without case sensitivity.
- Sort by **Largest first**, **Name**, or **Oldest modified**. Modified time is filesystem metadata, not the date an app or project was last used.
- The header checkbox selects or clears eligible, visible, parentless items. It does not automatically select app-associated data.
- Selections survive filtering and navigation between categories. Review includes selected items hidden by the current filter or page.
- **Clear** removes the entire selection. Starting another scan or removing a project root also clears it.
- Clicking an item name opens details; the checkbox controls selection.
- Details stay beside the results on desktop, including the 900 px minimum window width. On narrow layouts they scroll into view when opened. Long details have their own scrolling area, whose position is preserved when the UI updates. Closing details returns keyboard focus and visibility to the original row.
- **Keep files**, the dialog close button, or Escape closes the review without moving anything.

## Cleanup history and restore

History is saved locally and shown newest first. Each row records its original path, estimated size, operation date, status, and any error. Successful moves get **Restore** and Finder-reveal controls. Associated files have their own history rows and must be restored separately.

| Status shown | Meaning |
| --- | --- |
| Checking | A move was recorded before its final outcome was saved |
| In Trash | The moved item was located and its file identity matched |
| Not moved | The move failed while the original item remained in place, or an interrupted move had not happened |
| Needs inspection | The move's result could not be verified safely |
| Restoring | A restore was recorded before its final outcome was saved |
| Restored | The item returned to its original location |
| Unavailable | The expected item cannot currently be located in accessible Trash |

The app reconciles interrupted history at startup and before restoration. This is not continuous monitoring of Finder. If another program moves or empties an item, the displayed status can be stale until reconciliation runs.

Restoration requires the original parent folder to exist, the destination to be absent, and the recorded item to remain accessible in `~/.Trash`. It never overwrites or automatically creates missing parents. Close an owning app before restoring its data. Read [recovery troubleshooting](safety.md#troubleshooting) for conflicts and unavailable items.

## Settings and appearance

Project roots must resolve to directories inside your home folder, on the same device, outside `Library`, `Applications`, and `.Trash`. Paths containing `.git`, `.Trash`, or a recognized dependency-folder component (including timestamped/numbered copies) are rejected. Choosing a broader root replaces any narrower configured roots it contains; choosing a root already covered by another does not duplicate it.

Adding a folder in Settings requires another scan to include it. **Add folder & scan** on node_modules scans immediately when the configured folders change. Removing a folder removes it from scan settings and clears cached results; it does not remove that folder from disk.

**System**, **Light**, and **Dark** appearance are available. System follows the Mac's color preference. Desktop settings and history persist between launches. Browser-preview appearance lasts only for the current page session.

## macOS permissions

**Settings → Permissions** provides two native actions:

- **Open Privacy & Security**: open System Settings, choose **Files & Folders**, and allow needed Documents, Desktop, or Downloads access for Mac Cleaner. If the app is not listed, add the project folder through the native picker first.
- **Open Full Disk Access**: optionally add and enable Mac Cleaner for protected cache or Trash locations. Quit and reopen the app afterward, then scan again.

macOS requires the user to grant access; opening a panel does not enable it. This section does not claim to detect the current Full Disk Access state. Permissions do not add project roots or remove the scanner's scope restrictions. Scan notices include a **Review folders and permissions** shortcut to Settings. [Apple's folder-access guide](https://support.apple.com/en-ie/guide/mac-help/mchld5a35146/27/mac/27) describes the system controls.

## Keyboard, layout, and browser preview

Controls use native buttons, checkboxes, search, select, and dialog elements. Navigation and icon-only controls have accessible names. Checkbox focus is preserved across rendering; Space toggles a focused checkbox. Status messages and errors use live announcement roles. Reduced-motion styles are included.

The desktop window starts at 1180 × 800 and has a 900 × 620 minimum. Responsive browser layouts were checked down to 390 pixels wide. Native VoiceOver behavior remains unverified; see [validation](../VALIDATION.md).

`npm run dev` serves an empty browser preview. It supports navigation and appearance, but scanning, folder selection, Finder, cleanup, and restore require the desktop runtime. Synthetic scan data exists only inside the UI tests.
