# IPC and data reference

[Documentation index](README.md)

This is the internal frontend-to-Rust contract, not a network API. Commands are implemented in [main.rs](../src-tauri/src/main.rs), declared in [build.rs](../src-tauri/build.rs), and allowed for the `main` window by [capabilities/main.json](../src-tauri/capabilities/main.json).

JavaScript invokes snake_case command names with camelCase argument keys. Rust serializes model fields as camelCase. Errors from `Result<_, String>` reject the frontend promise; `()` results have no useful return value. Do not depend on exact error wording as a stable error code.

## Commands

| Command | JavaScript arguments | Success result | Behavior and failures |
| --- | --- | --- | --- |
| `bootstrap` | None | `{ settings: Settings, history: Record[] }` | Reconciles and saves history; rejects while an exclusive operation is busy or state cannot be saved |
| `scan` | `{ onEvent: Channel<ScanEvent> }` | `Summary` | Clears old registry, streams results, returns final summary; blocking worker and exclusive operation |
| `cancel_scan` | None | Void | Sets the cancellation flag; does not wait for completion; the next scan resets the flag |
| `choose_root` | None | `Settings` | Opens native directory picker, validates and deduplicates the chosen root, saves if changed; cancellation returns current settings |
| `remove_root` | `{ index: number }` | `Settings` | Zero-based settings-root index; rejects an invalid index; clears registry and saves |
| `set_appearance` | `{ appearance: 'system' \| 'light' \| 'dark' }` | `Settings` | Rejects other strings; persists appearance under the store lock |
| `trash_items` | `{ ids: string[] }` | `Record[]` | Current scan IDs only; 1–10,000 submitted entries; validates all before movement; returns the complete history, including per-item failures |
| `restore_item` | `{ id: string }` | `Record[]` | History ID, not scan ID; reconciles then restores one available item; may return a record with an error after a failed filesystem move |
| `reveal_item` | `{ id: string }` | Void | Resolves a scan ID first, then history ID; reveals the original location for restored history, otherwise its recorded Trash location |
| `open_update` | None | Void | Opens the fixed latest-release DMG URL in the default browser; accepts no caller URL; reports native launch failure |
| `open_trash` | None | Void | Opens `~/.Trash` in Finder; reports failure if the native request is rejected |
| `open_privacy_settings` | `{ fullDiskAccess: boolean }` | Void | Opens the fixed Full Disk Access URL when true, otherwise Privacy & Security; never enables permissions or accepts a caller-provided URL |

The command ACL grants `allow-<command-name-with-hyphens>` for each row. It also allows `core:window:allow-start-dragging`. Adding a command requires updating its Rust handler/registration, the build manifest, and the capability allowlist; generated permission files alone do not authorize a window.

The review acknowledgement is enforced by the interface, not an IPC argument. Backend safeguards still validate scope, identity, eligibility, overlap, parent association, device, and known running apps. Backend operations do not trust a frontend-provided path or size.

## Scan events

`src/types.ts` defines the frontend union; `scan.rs` defines the serialized Rust enum:

```ts
type ScanEvent =
  | { kind: 'item'; item: Item }
  | { kind: 'phase'; message: string }
  | { kind: 'warning'; message: string }
  | { kind: 'progress'; processed: number; total: number };
```

An `item` first arrives as a placeholder, then as an updated measurement result with the same ID. Consumers must replace by ID rather than append every event. Updates for different candidates can arrive in any measurement-worker order. No event automatically selects an item.

`phase` describes work, not a numeric percentage. Warnings are emitted after measurement and also returned in `Summary.warnings`; the frontend replaces its accumulated warning list with the final list to avoid duplicates. The command handler ignores channel-send failures, so losing the UI listener does not itself cancel the scan.

`progress` starts with `processed: 0` after discovery, when `total` is fixed. Each finished candidate check increments processed, including unsuccessful measurements; cancellation stops further increments. Concurrent workers can send counts in a different order from their increments, so the frontend retains the maximum processed count received. The bar shows an indeterminate discovery state before this event, caps running/stopped percentages at 99%, and shows 100% only after a non-cancelled command result. It measures checked candidates, not elapsed-time or byte completion. `total: 0` is valid.

Current frontend usage, reduced to the scan contract:

```ts
import { Channel, invoke } from '@tauri-apps/api/core';
import type { ScanEvent, Summary } from './types';

const onEvent = new Channel<ScanEvent>();
onEvent.onmessage = event => {
  // Update presentation state by event kind and item ID.
};
const summary = await invoke<Summary>('scan', { onEvent });
```

This snippet illustrates the existing code; it is not a separate entry point. See [main.ts](../src/main.ts) for selection resets, error handling, and rendering.

## Item

Rust `Item` is serialized to the frontend but is not persisted. The frontend's TypeScript interface intentionally declares only fields it uses; the serialized `identity` remains an additional field. Modification requests refer back to the Rust registry.

| Field | Wire type | Meaning |
| --- | --- | --- |
| `id` | string | Ephemeral scan candidate ID; a new scan creates new IDs |
| `name` | string | Display name, such as project name or installed-app name |
| `path` | string | Absolute candidate path |
| `root` | string | Stored scan scope; candidate must be strictly below it |
| `category` | string | `node`, `xcode`, `packages`, `caches`, or `apps` |
| `bytes` | number | Allocated-block estimate; initially zero |
| `modified` | number | Top-level modification time in Unix seconds, clamped at zero |
| `description` | string | What the item contains |
| `consequence` | string | What removal can affect |
| `complete` | boolean | Measurement completed and final top-level identity matched |
| `selectable` | boolean | Candidate is eligible under scan/category rules; the UI also requires `complete` and no active operation |
| `issue` | string or null | Reason an item is excluded or incomplete |
| `bundleId` | string or null | Owning application's identifier, used for running-app checks |
| `appParent` | string or null | Parent app's scan ID for associated data |
| `version` | string or null | App short version, if available |
| `identity` | `Identity` | Rust-captured metadata for later validation |

An incomplete placeholder may have `selectable: true`. Always require both flags for UI selection. A complete item may have `selectable: false`, for example an unknown cache owner.

## History record

Rust calls this model `Record`; the frontend calls its consumed subset `HistoryItem`. History IDs differ from scan IDs. Multiple selected items share an `operation` ID but retain independent outcomes.

| Field | Wire type | Meaning |
| --- | --- | --- |
| `id` | string | Persistent history record ID; accepted by `restore_item` |
| `operation` | string | Identifier grouping one cleanup submission |
| `name` | string | Display name captured at cleanup |
| `original` | string | Original absolute path, also the restore destination |
| `trashPath` | string or null | Resulting native Trash path, possibly recovered by reconciliation |
| `identity` | `Identity` | Identity captured before movement |
| `bytes` | number | Size estimate from the scan, not a live measurement of Trash |
| `timestamp` | number | Unix seconds when the pending cleanup record was created; not updated to the restore time |
| `status` | string | State listed below |
| `error` | string or null | Human-readable failure or inspection message |
| `bundleId` | string or null | Optional owning app, checked before restore |

`HistoryItem` omits `identity` and `bundleId` from its TypeScript declaration because the UI does not use them. Rust stores and returns them. App-parent relationships are not persisted in history; restoration is per record.

### History statuses

| Value | UI label | Normal origin or transition |
| --- | --- | --- |
| `pending` | Checking | Intent saved before the native move |
| `trashed` | In Trash | Native destination identity verified, or reconciliation found the item |
| `failed` | Not moved | Original identity remains after an error, associated app failed, or reconciliation found an interrupted-before-move intent |
| `uncertain` | Needs inspection | Original/destination identity could not establish the outcome |
| `restoring` | Restoring | Restore intent saved before the exclusive rename |
| `restored` | Restored | Restore succeeded, or reconciliation completed an interrupted restore |
| `unavailable` | Unavailable | Reconciliation could not locate an accessible matching Trash item |

Normal flow is `pending → trashed → restoring → restored`. A failed restore returns to `trashed` with an error. Reconciliation can recover an `uncertain` or `unavailable` item to `trashed` if it later becomes locatable. The UI offers Restore only for `trashed` records. Full algorithms are in [backend recovery](backend.md#history-reconciliation).

## Other models

| Model | Fields and rules |
| --- | --- |
| `Identity` | `dev: u64`, `ino: u64`, `modified: i64`, `nanos: i64`; full equality before cleanup, device/inode equality after movement |
| `Settings` | `roots: string[]`, `appearance: 'system' \| 'light' \| 'dark'`; defaults to no roots and system appearance |
| `Summary` | `elapsedMs: number`, `cancelled: boolean`, `count: number`, `warnings: string[]`; count includes all candidates, not just eligible ones |
| `ScanProgress` | Frontend model with `processed: number`, `total: number`; populated by progress events, not persisted |
| `Saved` | `{ settings: Settings, history: Record[] }`; the entire persisted document |
| `Bootstrap` | `{ settings: Settings, history: Record[] }`; the startup command result |
| `Category` | TypeScript key union derived from the `categories` object |
| `Page` | `Category \| 'overview' \| 'history' \| 'settings'` |

Rust serializes numeric counters/timestamps as JSON numbers. The frontend uses JavaScript `number`; it does not perform identity comparisons or send identity fields back for authorization. Byte formatting uses decimal units (1 KB = 1,000 bytes).

## Persisted state

Default location: `~/Library/Application Support/com.maccleaner.desktop/state.json`.

A fresh state has this shape:

```json
{
  "settings": {
    "roots": [],
    "appearance": "system"
  },
  "history": []
}
```

`state.lock` holds the process lock, and `state.json.tmp` is the temporary write target. Saves write the full document, sync it, atomically replace `state.json`, and sync the parent directory. A crash can leave a temporary file; loading reads `state.json`, not the temporary file.

Scan results, selection, filters, and scan summaries are not persisted. History has no pruning/export/clear command, encryption, or schema migration. It contains local filenames and paths; consider that before sharing the file for debugging. Avoid manual edits while the app is running.
