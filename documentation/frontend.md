# Frontend code

[Documentation index](README.md) · [Feature behavior](features.md)

The frontend uses TypeScript and the DOM directly. There is no React component tree, router, state-management package, or production demo-data source. [main.ts](../src/main.ts) renders the interface from one in-memory state object and delegates events from the application root.

`updates.ts` checks GitHub release metadata with a ten-second timeout. Startup checks run only on desktop and do not delay initialization. A toolbar button appears for a newer stable release with the expected DMG; Settings provides manual retry and status. `open_update` opens the fixed download URL in the default browser. The displayed version comes from the Tauri configuration through Vite.

## Entry points and state

[index.html](../index.html) provides three persistent elements:

- `#app`: sidebar, active screen, errors, and selection bar.
- `#review`: a native HTML dialog outside the main render tree.
- `#toast`: a polite live status region outside the main render tree.

`isTauri()` determines whether native commands are available. `init()` first renders the loading/empty state, calls `bootstrap` in the desktop runtime, assigns settings/history, marks the app ready, applies the appearance, and renders again. A bootstrap error remains visible and leaves scan controls disabled. Browser preview becomes ready without loading filesystem data.

| State field | Role |
| --- | --- |
| `page` | Active category, overview, history, or settings; no URL routing |
| `items` | Streamed scan items, updated by ID |
| `selected` | `Set<string>` of explicitly selected scan IDs |
| `settings`, `history` | Frontend copies of backend state |
| `busy` | Empty string, `folder`, `scan`, `trash`, or `restore`; drives disabled controls and progress |
| `summary` | Final scan result, initially null |
| `scanProgress` | Latest fixed total and maximum processed-item count; null during discovery and reset for each scan |
| `phase`, `warnings` | Live scan message and collected notices |
| `query`, `sort` | Current name/path filter and sort mode |
| `detail` | Scan ID of the item shown in the details pane, or null |
| `error` | Dismissible operation error |
| `permissions`, `permissionError`, `recheckPermissions` | Last per-folder access checks, check failure, and one pending refresh after returning from System Settings |
| `ready` | Whether bootstrap or browser initialization succeeded |

`reviewIds` is a separate snapshot of IDs taken when review opens. `renderTimer` batches scan rendering; `toastTimer` controls status-message dismissal. These are not persisted.

## Rendering functions in main.ts

| Function | Responsibility |
| --- | --- |
| `sidebar()` | Navigation, non-app cleanup total, category totals, tool links, version/local-processing note |
| `scanButton()` | Scan/scan-again button, or Stop scan while scanning |
| `toolbar()` | Page context, browser-preview label, scan control, drag region, followed by shared scan progress |
| `warningPanel()` | Expandable notices with scrollable details |
| `progress()` | Native progress element, percentage, phase and checked counts; discovery, running, completed, and stopped states |
| `overview()` | Storage heading, proportional chart, category cards, uninstall link, and Trash explanation |
| `visibleItems()` | Filter active category by case-insensitive name/path, then sort by size, name, or oldest modification; apps match their associated data too and reveal all children beneath checked parents |
| `detailPanel()` | Selected row's explanation, consequences, full path, issue, version, date, and Finder action |
| `categoryPage()` | Category heading, project-folder prompt or uninstall note, search/sort, selectable list, details |
| `historyPage()` | Reversed history, status labels, recorded errors, Finder/Restore controls, Open Trash |
| `settingsPage()` | Project-root list, add/remove controls, macOS access checks/results and permission links, appearance, local-processing facts |
| `footer()` | Total eligible selection, Clear, and Review; hidden on settings/history |
| `render()` | Replace the root markup, preserve scroll and control focus, restore the search caret |

The storage chart uses each category's estimated bytes as its flex weight. The selected proportion is rendered as a solid fill within the striped category segment. Both chart segments and legend entries navigate to their category.

`render()` saves `.main-scroll` position before replacing `app.innerHTML`. It also preserves the details pane's own scroll position when its `data-item-id` still matches the selected item; opening a different item resets that pane to the top. Search input focus/caret get special handling. Other focused controls are located using stable `data-*` attributes escaped with `CSS.escape`. When adding interactive elements, preserve a stable identity so rendering does not lose keyboard position.

`progress()` renders below every page toolbar during a scan and after a successful or cancelled result. Before discovery establishes a total, `<progress>` has no value and shows Finding items. Progress events update a monotonic processed count. The numeric bar uses the percentage of candidates checked, capped at 99% until the command returns normally. A completed scan, including an empty scan, shows 100%; a stopped scan retains its partial progress. Scan errors clear the running bar rather than presenting completion. The native progress element has an accessible name/value description, and discovery animation respects reduced motion.

### Small helpers

| Helper | Purpose |
| --- | --- |
| `notify()` | Writes toast text with `textContent`, shows it, and resets its 6.5-second dismissal timer |
| `total()` / `count()` | Category byte sum and candidate count; count excludes app-associated children |
| `selection()` | Resolves selected IDs to currently complete/selectable items |
| `canSelect()` | Requires complete, selectable, and no active operation |
| `date()` | Localized date from Unix seconds, or “Not available” |
| `tile()` | Category-colored icon markup |
| `disable()` | Disables operation controls while busy or not initialized |
| `applyAppearance()` | Writes the settings preference to `document.documentElement.dataset.theme` |
| `scheduleRender()` | Coalesces incoming scan-event renders with a 100 ms timer |

## Operations and event handlers

`startScan()` refuses filesystem work in a browser preview. In the desktop runtime it calls `addRoot()` first when no project root is configured. A picker error aborts scanning and releases the folder busy state. Cancelling continues with the other categories; node_modules shows Not scanned and the backend returns a notice. It then resets previous results, selected IDs, details, summary, warnings, and error. A Tauri `Channel` updates items by ID and handles phase/warning messages. The final command result supplies the summary and canonical warning list. A `finally` block clears the busy state even on failure.

`showReview()` takes a fresh selection snapshot and builds the dialog from exact names, paths, estimates, and consequences. Confirm starts disabled until the affected-apps/builds acknowledgement is checked. The dialog sits outside `#app`, so normal root renders do not replace it.

`confirmTrash()` rechecks acknowledgement/busy state, closes the dialog, and invokes `trash_items` with `reviewIds`. On success it replaces history, removes rows at paths represented by trashed history, clears selection, and navigates to history. On command rejection it keeps the error and attempts `bootstrap` reconciliation; successful reconciliation clears potentially stale scan rows and shows history. If reconciliation also fails, the original operation error is retained.

`addRoot()` holds the frontend `folder` busy state while invoking the native picker, assigns returned settings, and returns whether the roots changed. It always releases busy in `finally`. The node_modules **Add folder & scan** handler starts a scan when a folder was added; Settings only updates roots. A cancelled picker returns false without a misleading success notification.

The delegated handlers cover all remaining interactions:

| Event | Behavior |
| --- | --- |
| Search `input` | Update query and render while preserving input focus/caret |
| Sort `change` | Update sort and render |
| Item checkbox `change` | Select an app together with eligible detected associated data; allow individual child opt-out; reject associated uninstall data without its app; user caches remain independently selectable; deselect children with their app |
| Visible-items checkbox `change` | Select/clear eligible visible items; on the uninstall screen include each app's eligible children through the same selection helper |
| Navigation button `click` | Change page, clear query/details, render, and scroll to top; retain selection and sort |
| Appearance button `click` | Save through `set_appearance` on desktop; update memory only in preview |
| Scan/cancel buttons | Start scanning or send `cancel_scan` and display “Stopping scan…” |
| Remove-root button | Remove by index, replace settings, clear results/selection/summary |
| Details buttons | Open and scroll the pane into view, focus Close details; closing returns focus/visibility to the original row |
| Finder/Trash buttons | Invoke ID-based reveal or open Trash; show an availability message in preview |
| Permission buttons | Request/check invokes `check_permissions`, holds the `permissions` busy state, and reports loading/results/failures; System Settings invokes `open_privacy_settings` with `fullDiskAccess` false or true and marks access for rechecking on window focus; preview explains native availability |
| Review/Clear buttons | Open exact-selection review or clear selection |
| Restore button | Set busy, invoke by history ID, update history, report the returned per-item result, clear busy |
| Dismiss-error button | Clear the error banner |
| Drag-region `mousedown` | Ask the current Tauri window to start dragging when the exact target carries `data-drag` |
| Dialog `change` / `click` | Enable confirmation after acknowledgement; close or confirm the dialog |

The native dialog supplies Escape dismissal. No custom destructive keyboard shortcut is registered.

## Types, formatting, and icons

[types.ts](../src/types.ts) declares `Item`, `HistoryItem`, `Settings`, `Summary`, `ScanProgress`, `ScanEvent`, `Category`, and `Page`. The shared `categories` object supplies names, icons, descriptions, and summary consequences. The Rust backend supplies each individual item's actual description and consequence. Wire fields are detailed in [the API reference](api.md).

| Export | Behavior |
| --- | --- |
| `bytes()` | Decimal B/KB/MB/GB/TB formatting with English number formatting; non-finite/nonpositive values become `0 B` |
| `escape()` | HTML-escapes `&`, `<`, `>`, double quotes, and apostrophes after converting a value to text |
| `selectedItems()` | Filters item objects by selected ID, completion, and eligibility |
| `storageTotal()` | Sums item byte estimates |

All filesystem-derived names, paths, descriptions, issues, and history errors interpolated into HTML must pass through `escape()`. Toasts use `textContent`. Keep CSS/SVG identifiers derived from trusted local definitions; `escape()` is an HTML helper, not a general-purpose sanitizer for scripts or URLs.

[icons.ts](../src/icons.ts) keeps fixed SVG path markup for navigation and actions. `icon(name, className)` returns a 20 × 20 SVG with a 24 × 24 view box, `currentColor`, rounded strokes, and `aria-hidden="true"`. Unknown names use the information icon. Controls supply their own accessible names. There are no remote fonts or remote icon downloads.

## Styles and appearance

[style.css](../src/style.css) imports [tokens.css](../src/tokens.css). The visual reference is [example-image.png](../example-image.png); design rationale lives in [DESIGN.md](../DESIGN.md).

`tokens.css` defines background, surfaces, text, borders, controls, focus, and status colors. Each category sets its own accent variables: green/teal dependencies, purple Xcode, blue-green package caches, amber app caches, and coral applications. Dark is the base palette; explicit light and a light-system media query override it. Explicit dark takes precedence over the system's light preference.

`style.css` owns the two-column app frame, sidebar, overview/chart/cards, results and details, history, settings, selection bar, dialog, toasts, errors, and notice states. The system font stack matches macOS typography without loading a webfont.

| Viewport rule | Main effect |
| --- | --- |
| Default | 244 px sidebar; two-column category cards; side-by-side details where space permits |
| At most 1060 px | 211 px sidebar, tighter content; details remain beside the list |
| At most 700 px | 66 px icon sidebar, one-column cards, simplified rows, larger action targets; details stack after the list and scroll into view on open |
| At least 1500 px | Keeps the floating selection bar aligned with bounded content |
| Reduced motion | Removes transitions and smooth scrolling |

On desktop, `.details-pane` is sticky within `.main-scroll`, aligned to the start of its grid area, and has a viewport-bounded height with its own overflow scrolling. `.results-layout` must not have `overflow: hidden`, which would interfere with that sticky scrolling ancestor; rounded clipping is applied to `.results-list` instead. Scroll margins leave room for the floating selection bar. Closing details restores the original row even when a narrow layout required scrolling to the end of the list.

Use `.content.overview` for overview layout adjustments: `.overview` is also used on navigation icons as a category color class. Broad layout rules on that shared class affect the sidebar.

## Accessibility and validation boundaries

Native buttons, checkboxes, search/select controls, and `<dialog>` provide keyboard behavior. Focus-visible outlines, navigation `aria-current`, accessible action labels, appearance `aria-pressed`, live statuses, error alerts, and descriptive storage-segment labels are implemented. Narrow navigation keeps accessible names when visible text is hidden.

The browser tests exercise first-scan folder selection, cancellation/error recovery, permission actions, keyboard checkbox selection, focus retention, dialog cancellation, escaped filenames, and narrow layouts. They do not establish complete accessibility compliance, native System Settings navigation, or native VoiceOver support. See [test coverage](development.md#testing) and [recorded validation](../VALIDATION.md).
