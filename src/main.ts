import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { bytes, categories, escape as e, selectedItems, storageTotal, type AccessCheck, type Category, type HistoryItem, type Item, type Page, type ScanEvent, type ScanProgress, type Settings, type Summary } from './types';
import { icon } from './icons';
import { checkUpdate } from './updates';
import './style.css';

const desktop = isTauri();
const app = document.querySelector<HTMLDivElement>('#app')!;
const dialog = document.querySelector<HTMLDialogElement>('#review')!;
const state = {
  page: 'overview' as Page, items: [] as Item[], selected: new Set<string>(),
  settings: { roots: [], appearance: 'system' } as Settings, history: [] as HistoryItem[],
  busy: '' as '' | 'folder' | 'scan' | 'quit' | 'trash' | 'restore' | 'permissions', summary: null as Summary | null,
  scanProgress: null as ScanProgress | null,
  phase: '', warnings: [] as string[], query: '', sort: 'size', detail: null as string | null,
  error: '', ready: false,
  updateVersion: null as string | null, checkingUpdate: false, updateStatus: '',
  permissions: null as AccessCheck[] | null, permissionError: '', recheckPermissions: false,
};
let toastTimer: ReturnType<typeof setTimeout>;
let renderTimer: ReturnType<typeof setTimeout> | undefined;
let reviewIds: string[] = [];

function notify(message: string) {
  const toast = document.querySelector<HTMLDivElement>('#toast')!;
  toast.textContent = message; toast.classList.add('visible');
  clearTimeout(toastTimer); toastTimer = setTimeout(() => toast.classList.remove('visible'), 6500);
}
function total(category: string) { return storageTotal(state.items.filter(i => i.category === category || category === 'apps' && i.appParent)); }
function count(category: string) { return state.items.filter(i => i.category === category && (category !== 'apps' || !i.appParent)).length; }
function selection() { return selectedItems(state.items, state.selected); }
function canSelect(item: Item) { return item.selectable && item.complete && !state.busy; }
function selectItem(item: Item, checked: boolean) {
  for (const candidate of [item, ...state.items.filter(i => i.appParent === item.id)]) {
    if (checked && canSelect(candidate)) state.selected.add(candidate.id);
    else state.selected.delete(candidate.id);
  }
}
function date(time: number) { return time ? new Date(time * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }) : 'Not available'; }
function tile(key: Category) { return `<span class="tile ${key}">${icon(categories[key].icon)}</span>`; }
function disable() { return state.busy || !state.ready ? 'disabled' : ''; }
function applyAppearance() { document.documentElement.dataset.theme = state.settings.appearance; }
function scheduleRender() { if (!renderTimer) renderTimer = setTimeout(() => { renderTimer = undefined; render(); }, 100); }

function sidebar() {
  const nav = (page: Page, name: string, glyph: string, value = '', category = '') => `<button class="nav-item ${state.page === page ? 'active' : ''}" data-page="${page}" aria-label="${name}" ${state.page === page ? 'aria-current="page"' : ''}><span class="nav-icon ${category || page}">${icon(glyph)}</span><span>${name}</span>${value ? `<span class="nav-size">${value}</span>` : ''}</button>`;
  return `<aside class="sidebar"><div class="window-space" data-drag><span class="product-name">Mac Cleaner</span></div><nav aria-label="Main navigation">
    ${nav('overview', 'Overview', 'overview', state.summary || state.items.length ? bytes(storageTotal(state.items.filter(i => i.category !== 'apps'))) : '')}
    <p class="nav-label">Review storage</p>
    ${(['node', 'xcode', 'packages', 'caches'] as Category[]).map(key => nav(key, categories[key].name, categories[key].icon, key === 'node' && !state.settings.roots.length && state.summary ? 'Not scanned' : state.items.length ? bytes(total(key)) : '', key)).join('')}
    <p class="nav-label tools-label">Tools</p>
    ${nav('apps', 'Uninstall an app', 'app', '', 'apps')}
    ${nav('history', 'Cleanup history', 'history')}
    ${nav('settings', 'Settings', 'settings')}
  </nav><div class="sidebar-bottom">${icon('shield')}<span>Your files stay on your Mac.<small>Mac Cleaner · ${e(__APP_VERSION__)}</small></span></div></aside>`;
}

function scanButton() {
  return state.busy === 'scan'
    ? `<button class="button secondary" data-action="cancel">Stop scan</button>`
    : `<button class="button scan-button" data-action="scan" ${disable()}>${icon('scan')} ${state.summary ? 'Scan again' : 'Scan my Mac'}</button>`;
}
function toolbar(title: string) {
  return `<header class="toolbar" data-drag><span>${title}</span><div>${!desktop ? '<span class="preview-label">Browser preview</span>' : ''}${state.updateVersion ? '<button class="button secondary" data-action="update">Update available</button>' : ''}${scanButton()}</div></header>${progress()}`;
}

async function refreshUpdate() {
  if (!desktop || state.checkingUpdate) return;
  state.checkingUpdate = true; state.updateStatus = 'Checking for updates…'; render();
  try {
    state.updateVersion = await checkUpdate();
    state.updateStatus = state.updateVersion ? `Version ${state.updateVersion} is available. Open the DMG, quit Mac Cleaner, then drag the new app into Applications.` : 'You’re up to date. No newer stable DMG is available.';
  } catch {
    state.updateStatus = 'Could not check for updates. Check your connection and try again.';
  } finally { state.checkingUpdate = false; render(); }
}
function warningPanel() {
  return state.warnings.length ? `<details class="warnings"><summary>${icon('info')} ${state.warnings.length} scan notice${state.warnings.length === 1 ? '' : 's'}. Some locations could not be scanned.</summary><ul>${state.warnings.map(w => `<li>${e(w)}</li>`).join('')}</ul><button class="button secondary" data-page="settings">Review folders and permissions</button></details>` : '';
}

async function refreshPermissions() {
  if (!desktop) { notify('macOS permissions are available in the desktop app.'); return; }
  if (state.busy) return;
  state.busy = 'permissions'; state.permissions = null; state.permissionError = ''; render();
  try { state.permissions = await invoke<AccessCheck[]>('check_permissions'); }
  catch (error) { state.permissionError = String(error); }
  finally { state.busy = ''; render(); }
}
function progress() {
  if (state.busy !== 'scan' && !state.summary) return '';
  const finished = !!state.summary && !state.summary.cancelled;
  const stopped = state.summary?.cancelled;
  const checked = state.scanProgress;
  const percent = finished ? 100 : checked ? Math.min(99, checked.total ? Math.floor(checked.processed / checked.total * 100) : 0) : null;
  const label = finished ? (state.warnings.length ? 'Scan complete with notices' : 'Scan complete') : stopped ? 'Scan stopped' : checked && checked.processed === checked.total ? 'Finishing scan…' : state.phase || 'Finding items…';
  const counts = finished ? `${state.summary!.count} items checked` : checked ? `${checked.processed} of ${checked.total} items checked` : `${state.items.length} items found · total not known yet`;
  return `<section class="scan-progress" aria-label="Scan status"><div class="scan-progress-heading"><span id="scan-phase" aria-live="polite">${e(label)}</span><strong>${percent === null ? (stopped ? 'Stopped' : 'Finding items…') : `${percent}%`}</strong></div><progress max="100" ${percent !== null || stopped ? `value="${percent ?? 0}"` : ''} aria-label="Scan progress" aria-describedby="scan-phase" aria-valuetext="${e(percent === null ? (stopped ? 'Stopped during discovery' : 'Finding items; total not known yet') : `${percent}% · ${counts}`)}"></progress><p>${counts}</p></section>`;
}

function overview() {
  const cleanup = state.items.filter(i => i.category !== 'apps');
  const sum = storageTotal(cleanup);
  const scanned = !!state.summary || state.items.length > 0;
  const keys = ['node', 'xcode', 'packages', 'caches'] as Category[];
  const selected = selection();
  const elapsed = state.summary ? `${(state.summary.elapsedMs / 1000).toFixed(1)} seconds` : '';
  return `${toolbar('Storage overview')}<div class="overview content">
    <section class="storage-heading"><p class="eyebrow">${scanned ? 'Mac Cleaner found' : 'A little space goes a long way.'}</p>
      <h1 class="${scanned ? 'storage-number' : 'welcome-title'}">${scanned ? bytes(sum) : 'Make room for<br>what’s next.'}</h1>
      <p class="subtitle">${scanned ? `${state.busy === 'scan' ? 'Still scanning' : state.summary?.cancelled ? 'Scan stopped' : state.warnings.length ? 'Scan finished with notices' : 'Ready to review'}${elapsed ? ` · ${elapsed}` : ''} · ${cleanup.length} items found` : 'Find build data, old dependencies, and caches.<br>Understand every item. Keep the final say.'}</p>
    </section>
    <section class="storage-breakdown" aria-label="Cleanup storage by category">
      <div class="storage-track ${sum ? '' : 'empty-track'}">${sum ? keys.filter(key => total(key) > 0).map(key => {
        const amount = total(key); const chosen = storageTotal(selected.filter(i => i.category === key));
        return `<button class="storage-segment ${key}" style="flex-grow:${amount};--chosen:${Math.min(chosen / amount * 100, 100)}%" data-page="${key}" aria-label="${categories[key].name}: ${bytes(amount)}, ${bytes(chosen)} selected"><span></span></button>`;
      }).join('') : '<span>Storage breakdown appears after your scan</span>'}</div>
      <div class="storage-legend">${sum ? keys.filter(key => total(key) > 0).map(key => `<button data-page="${key}"><i class="${key}"></i>${categories[key].name}<span>${bytes(total(key))}</span></button>`).join('') : `<span>${icon('info')} Your files stay exactly where they are while scanning.</span>`}</div>
    </section>
    <div class="choice-note">${icon('shield')}<div><strong>You choose what goes.</strong><p>Nothing is selected automatically. Everything you remove goes to Trash first.</p></div>${scanned ? `<button class="button quiet" data-page="${cleanup.find(i => i.selectable)?.category || 'node'}">Review items</button>` : ''}</div>
    <div class="section-title"><h2>Take a closer look</h2><span>${scanned ? 'Storage on disk, estimated' : 'Four places to start'}</span></div>
    <div class="category-grid">${keys.map(key => `<button class="category-card ${key}" data-page="${key}">${tile(key)}<div class="card-copy"><div class="card-title"><h3>${categories[key].name}</h3><strong>${key === 'node' && !state.settings.roots.length ? (scanned ? 'Not scanned' : '') : scanned ? bytes(total(key)) : ''}</strong></div><p>${categories[key].description}</p><span class="card-meta">${key === 'node' && !state.settings.roots.length ? 'Choose your project folders' : scanned ? `${count(key)} ${key === 'node' ? 'projects' : 'items'}` : 'Ready to scan'} ${icon('arrow')}</span></div></button>`).join('')}</div>
    <button class="uninstall-link" data-page="apps"><span class="tile apps">${icon('app')}</span><span><strong>Some apps have overstayed their welcome?</strong><small>Review apps and the files they leave behind.</small></span>${icon('arrow')}</button>
    ${warningPanel()}
    <p class="storage-footnote">Moving files to Trash keeps them recoverable. Space becomes available after you empty Trash in Finder.</p>
  </div>`;
}

function visibleItems() {
  const query = state.query.toLowerCase();
  const matches = (item: Item) => !query || `${item.name} ${item.path}`.toLowerCase().includes(query);
  const items = state.items.filter(item => item.category === state.page || state.page === 'apps' && item.appParent)
    .sort((a, b) => state.sort === 'name' ? a.name.localeCompare(b.name) : state.sort === 'modified' ? a.modified - b.modified : b.bytes - a.bytes);
  if (state.page !== 'apps') return items.filter(matches);
  return items.filter(item => !item.appParent && (matches(item) || items.some(child => child.appParent === item.id && matches(child))))
    .flatMap(item => [item, ...(state.selected.has(item.id) ? items.filter(child => child.appParent === item.id) : [])]);
}

function detailPanel() {
  const item = state.items.find(i => i.id === state.detail);
  if (!item) return '';
  return `<aside class="details-pane" data-item-id="${e(item.id)}" aria-label="Item details"><div class="details-top">${tile(item.category as Category)}<button class="icon-button" data-action="close-detail" aria-label="Close details">${icon('close')}</button></div><h2>${e(item.name)}</h2><p class="detail-size">${item.complete ? bytes(item.bytes) : 'Not fully measured'}</p><label>Location</label><p class="file-path">${e(item.path)}</p><button class="button secondary finder-button" data-action="reveal" data-id="${e(item.id)}">${icon('folder')} Show in Finder</button><label>What is it?</label><p>${e(item.description)}</p><label>When you remove it</label><p>${e(item.consequence)}</p>${item.issue ? `<p class="item-notice">${icon('info')} ${e(item.issue)}</p>` : ''}${item.version ? `<label>Version</label><p>${e(item.version)}</p>` : ''}<label>Modified</label><p>${date(item.modified)}</p></aside>`;
}

function categoryPage(key: Category) {
  const category = categories[key]; const items = visibleItems();
  const selectable = items.filter(i => canSelect(i) && (key !== 'apps' || !i.appParent));
  const allSelected = selectable.length > 0 && selectable.every(i => state.selected.has(i.id));
  return `${toolbar(key === 'apps' ? 'Application manager' : 'Review storage')}<div class="content category-content"><div class="page-heading">${tile(key)}<div><h1>${category.name}</h1><p>${category.description}</p></div><strong>${bytes(total(key))}</strong></div>
    ${key === 'node' ? `<div class="folder-notice">${icon('folder')}<span>${state.settings.roots.length ? `${state.settings.roots.length} project folder${state.settings.roots.length === 1 ? '' : 's'} included in scans` : 'node_modules is not scanned until you choose a project folder.'}</span><button class="button secondary" data-action="add-root" ${disable()}>${icon('plus')} Add folder & scan</button></div>` : ''}
    ${key === 'apps' ? '<p class="inline-note">Selecting an app also selects its detected support files, caches, settings, and other associated data. Review the paths below it and uncheck anything you want to keep, including local documents or sign-in data. Apps with extensions or privileged helpers may need their own uninstaller.</p>' : ''}
    <div class="list-controls"><label class="search-field">${icon('search')}<input id="search" type="search" placeholder="Search names or paths" value="${e(state.query)}" aria-label="Search names or paths"></label><label class="sort-control">Sort by <select id="sort" aria-label="Sort items"><option value="size" ${state.sort === 'size' ? 'selected' : ''}>Largest first</option><option value="name" ${state.sort === 'name' ? 'selected' : ''}>Name</option><option value="modified" ${state.sort === 'modified' ? 'selected' : ''}>Oldest modified</option></select></label></div>
    <div class="results-layout ${state.detail ? 'with-details' : ''}"><div class="results-list"><div class="list-header"><label><input type="checkbox" data-action="select-visible" ${allSelected ? 'checked' : ''} ${!selectable.length ? 'disabled' : ''} aria-label="Select ${selectable.length} visible items"><span>${items.length} items</span></label><span>On disk</span><span>Modified</span></div>
    ${items.length ? items.map(item => `<div class="item-row ${state.detail === item.id ? 'focused' : ''} ${item.appParent ? 'related' : ''}"><input type="checkbox" data-select="${e(item.id)}" aria-label="Select ${e(item.name)}" ${state.selected.has(item.id) ? 'checked' : ''} ${!canSelect(item) ? 'disabled' : ''}><button class="item-name" data-action="detail" data-id="${e(item.id)}"><span>${e(item.name)}${item.appParent ? '<small class="related-label">Associated data</small>' : ''}</span><small title="${e(item.path)}">${e(item.path)}</small>${item.issue ? `<small class="item-warning">${e(item.issue)}</small>` : ''}</button><strong>${item.complete ? bytes(item.bytes) : state.busy === 'scan' && !item.issue ? 'Calculating…' : 'Incomplete'}</strong><span class="modified">${date(item.modified)}</span></div>`).join('') : `<div class="empty-state">${icon(key === 'apps' ? 'app' : 'folder')}<h2>${state.query ? 'No matching items' : key === 'node' && !state.settings.roots.length ? 'Choose a project folder' : state.busy === 'scan' ? 'Looking for items…' : state.summary ? 'Nothing found here' : 'Let’s see what’s here'}</h2><p>${state.query ? 'Try another name or part of a path.' : key === 'node' && !state.settings.roots.length ? 'Add a project folder, then scan to find its dependencies.' : 'Run a scan to see the items you can review.'}</p>${!state.query && !state.busy ? scanButton() : ''}</div>`}
    </div>${detailPanel()}</div>${warningPanel()}</div>`;
}

function historyPage() {
  const labels: Record<string, string> = { trashed: 'In Trash', restored: 'Restored', failed: 'Not moved', unavailable: 'Unavailable', pending: 'Checking', uncertain: 'Needs inspection', restoring: 'Restoring' };
  return `${toolbar('Cleanup history')}<div class="content"><div class="section-title history-heading"><div><h1>A way back.</h1><p>Review what moved. Restore it while it’s still in Trash.</p></div><button class="button secondary" data-action="open-trash">${icon('trash')} Open Trash</button></div>
    ${state.history.length ? `<div class="history-list">${[...state.history].reverse().map(item => `<article class="history-row"><span class="history-mark ${item.status}">${icon(item.status === 'restored' ? 'check' : 'trash')}</span><div><h3>${e(item.name)} <span class="status-label">${labels[item.status] || e(item.status)}</span></h3><p class="file-path">${e(item.original)}</p><small>${bytes(item.bytes)} · ${date(item.timestamp)}</small>${item.error ? `<p class="item-warning">${e(item.error)}</p>` : ''}</div><div class="history-actions">${item.trashPath ? `<button class="icon-button" data-action="reveal" data-id="${e(item.id)}" aria-label="Reveal ${e(item.name)}">${icon('folder')}</button>` : ''}${item.status === 'trashed' ? `<button class="button secondary" data-action="restore" data-id="${e(item.id)}" ${disable()}>Restore</button>` : ''}</div></article>`).join('')}</div>` : `<div class="empty-state history-empty">${icon('history')}<h2>Your cleanup history starts here</h2><p>Items you move to Trash will appear here, with an option to restore them.</p><button class="button secondary" data-page="overview">Back to overview</button></div>`}
    <p class="storage-footnote">Restoration never replaces an existing file. Items emptied from Trash cannot be recovered by Mac Cleaner.</p></div>`;
}

function settingsPage() {
  return `${toolbar('Settings')}<div class="content settings-content"><h1>Make yourself at home.</h1><p class="subtitle">A few preferences. Everything stays on this Mac.</p>
    <section class="settings-section" aria-labelledby="updates-title"><h2 id="updates-title">Application updates</h2><p>Installed version: ${e(__APP_VERSION__)}. Checks GitHub for new releases; your files and cleanup history stay on this Mac.</p><div class="permission-row"><div><p aria-live="polite">${e(desktop ? state.updateStatus || 'Check for a newer stable release.' : 'Update checks are available in the desktop app.')}</p></div><button class="button secondary" data-action="check-update" ${!desktop || state.checkingUpdate ? 'disabled' : ''}>${state.checkingUpdate ? 'Checking…' : 'Check for updates'}</button></div></section>
    <section class="settings-section"><div class="section-title"><div><h2>Project folders</h2><p>Look for node_modules only where you choose.</p></div><button class="button secondary" data-action="add-root" ${disable()}>${icon('plus')} Add folder</button></div><div class="root-list">${state.settings.roots.length ? state.settings.roots.map((root, index) => `<div>${icon('folder')}<span class="file-path">${e(root)}</span><button class="icon-button" data-action="remove-root" data-index="${index}" aria-label="Remove ${e(root)} from scan folders" ${disable()}>${icon('close')}</button></div>`).join('') : '<p>No project folders added yet. Your home folder is never searched recursively by default.</p>'}</div></section>
    <section class="settings-section permissions-section" aria-labelledby="permissions-title"><h2 id="permissions-title">Permissions</h2><p>Allow Mac Cleaner to read the folders you want to scan. macOS manages these permissions in System Settings.</p>
      <div class="permission-row"><div><h3>Files & Folders</h3><p>Request access to Desktop, Documents, and Downloads through macOS, and check your project folders, app caches, and Trash. Approve any macOS prompts. If access was denied before, enable it in Privacy & Security → Files & Folders.</p></div><div class="permission-actions"><button class="button secondary" data-action="check-permissions" ${disable()}>${state.busy === 'permissions' ? 'Checking access…' : 'Request & check access'}</button><button class="button secondary" data-action="privacy-settings" ${disable()}>Open Privacy & Security</button></div></div>
      <div class="permission-checks" role="status" aria-live="polite" aria-busy="${state.busy === 'permissions'}">${state.busy === 'permissions' ? '<p>Checking folder access. Respond to any macOS permission prompts to continue.</p>' : state.permissionError ? `<p>Failed: could not check access. ${e(state.permissionError)} Try again.</p>` : state.permissions ? state.permissions.map(check => `<div class="permission-row"><div><h3>${e(check.name)}</h3><p class="file-path">${e(check.path)}</p><p>${e(check.message)}</p></div><strong class="permission-status" data-status="${check.status}">${check.status === 'success' ? 'Success' : check.status === 'failed' ? 'Failed' : 'Not available'}</strong></div>`).join('') : '<p>Not checked yet. Request & check access to see Success or Failed for each folder.</p>'}</div>
      <div class="permission-row"><div><h3>Full Disk Access <span class="permission-optional">Optional</span></h3><p>Open Full Disk Access and use + to add Mac Cleaner, or drag the installed app from Finder into the list, then enable it. Finder selects the app for you when it is running from an app bundle. Quit and reopen Mac Cleaner if macOS requests it.</p></div><button class="button secondary" data-action="full-disk-access" ${disable()}>Open Full Disk Access</button></div>
      <p class="permission-note">Only you can grant access. We recheck when you return from System Settings. Success means that folder could be listed at the last check; nested files and write access can still differ. These checks do not verify the Full Disk Access switch. Project folders still need to be added above.</p>
    </section>
    <section class="settings-section appearance-section"><div><h2>Appearance</h2><p>Follow your Mac, or choose a look.</p></div><div class="segmented" role="group" aria-label="Appearance">${(['system', 'light', 'dark'] as const).map(theme => `<button data-theme="${theme}" aria-pressed="${state.settings.appearance === theme}">${theme[0].toUpperCase() + theme.slice(1)}</button>`).join('')}</div></section>
    <section class="settings-section"><h2>Built around your choices</h2><div class="settings-fact">${icon('shield')}<div><strong>Local processing</strong><p>Scans and cleanup history stay on your Mac. No account, uploads, or telemetry.</p></div></div><div class="settings-fact">${icon('trash')}<div><strong>Trash comes first</strong><p>Mac Cleaner never empties Trash. Open it in Finder when you’re ready to permanently remove its contents.</p></div></div><div class="settings-fact">${icon('info')}<div><strong>Access is specific</strong><p>Inaccessible folders appear in scan notices. Protected apps and other volumes are excluded from cleanup.</p></div></div></section></div>`;
}

function footer() {
  const selected = selection();
  if (state.page === 'settings' || state.page === 'history') return '';
  return `<div class="selection-footer"><span class="selection-icon">${icon('trash')}</span><div><strong>${selected.length} ${selected.length === 1 ? 'item' : 'items'} selected</strong><span>${selected.length ? `${bytes(storageTotal(selected))} · moves to Trash` : 'Only what you choose'}</span></div>${selected.length ? '<button class="button quiet clear-button" data-action="clear">Clear</button>' : ''}<button class="button primary" data-action="review" ${!selected.length || state.busy ? 'disabled' : ''}>${state.busy === 'trash' ? 'Moving to Trash…' : `Review${selected.length ? ` ${bytes(storageTotal(selected))}` : ' selection'}`}</button></div>`;
}

function render() {
  const focused = document.activeElement;
  const searchFocused = focused instanceof HTMLInputElement && focused.id === 'search';
  const cursor = searchFocused ? focused.selectionStart : null;
  const focusSelector = focused instanceof HTMLElement && app.contains(focused)
    ? ['data-page', 'data-select', 'data-theme', 'data-action', 'data-id', 'data-index'].filter(attr => focused.hasAttribute(attr)).map(attr => `[${attr}="${CSS.escape(focused.getAttribute(attr)!)}"]`).join('')
    : '';
  const scroll = document.querySelector('.main-scroll')?.scrollTop || 0;
  const previousPane = app.querySelector<HTMLElement>('.details-pane');
  const detailScroll = previousPane?.dataset.itemId === state.detail ? previousPane.scrollTop : 0;
  app.innerHTML = `${sidebar()}<main class="main-scroll">${state.error ? `<div class="error-banner" role="alert">${icon('info')}<span>${e(state.error)}</span><button class="icon-button" data-action="dismiss-error" aria-label="Dismiss error">${icon('close')}</button></div>` : ''}${state.page === 'overview' ? overview() : state.page === 'history' ? historyPage() : state.page === 'settings' ? settingsPage() : categoryPage(state.page)}${footer()}</main>`;
  document.querySelector('.main-scroll')!.scrollTop = scroll;
  const pane = app.querySelector<HTMLElement>('.details-pane');
  if (pane) pane.scrollTop = detailScroll;
  if (searchFocused) { const input = document.querySelector<HTMLInputElement>('#search'); input?.focus(); if (cursor !== null) input?.setSelectionRange(cursor, cursor); }
  else if (focusSelector) { app.querySelector<HTMLElement>(focusSelector)?.focus({ preventScroll: true }); }
}

async function startScan() {
  if (!desktop) { notify('Open the Mac Cleaner desktop app to scan your Mac. This browser preview has no filesystem access.'); return; }
  if (state.busy) return;
  if (!state.settings.roots.length) await addRoot();
  state.busy = 'scan'; state.items = []; state.selected.clear(); state.detail = null;
  state.warnings = []; state.error = ''; state.summary = null; state.scanProgress = null; state.phase = 'Starting scan…'; render();
  const channel = new Channel<ScanEvent>();
  channel.onmessage = event => {
    if (event.kind === 'item') {
      const index = state.items.findIndex(item => item.id === event.item.id);
      if (index >= 0) state.items[index] = event.item; else state.items.push(event.item);
    } else if (event.kind === 'progress') {
      state.scanProgress = { processed: Math.max(state.scanProgress?.processed ?? 0, event.processed), total: event.total };
    } else if (event.kind === 'phase') { state.phase = event.message; }
    else { state.warnings.push(event.message); }
    scheduleRender();
  };
  try { state.summary = await invoke<Summary>('scan', { onEvent: channel }); state.warnings = state.summary.warnings; }
  catch (error) { state.error = String(error); }
  finally { state.busy = ''; render(); }
}

function showReview() {
  const chosen = selection(); if (!chosen.length || state.busy) return;
  reviewIds = chosen.map(i => i.id);
  dialog.innerHTML = `<div class="dialog-heading"><span class="tile history">${icon('trash')}</span><button class="icon-button" data-dialog="close" aria-label="Close review">${icon('close')}</button></div><h2 id="review-title">Move ${chosen.length} ${chosen.length === 1 ? 'item' : 'items'} to Trash?</h2><p>${bytes(storageTotal(chosen))} selected. You can restore these items from History while they remain in Trash.</p><div class="review-list">${chosen.map(item => `<div><strong>${e(item.name)}<span>${bytes(item.bytes)}</span></strong><small class="file-path">${e(item.path)}</small><p>${e(item.consequence)}</p></div>`).join('')}</div>${desktop && chosen.some(item => item.bundleId) ? '<section class="quit-apps"><button class="button secondary" data-dialog="quit">Quit selected apps</button><p id="quit-status" class="dialog-footnote" role="status">Ask the apps that own these items to quit. Save any open work before continuing.</p></section>' : ''}<label class="acknowledgement"><input id="acknowledge" type="checkbox">I’ve quit affected apps and stopped builds or installations.</label><p class="dialog-footnote">Disk space becomes available after you empty Trash in Finder.</p><div class="dialog-actions"><button class="button secondary" data-dialog="close">Keep files</button><button class="button primary" data-dialog="confirm" disabled>Move ${chosen.length} to Trash</button></div>`;
  dialog.showModal();
}

async function quitSelectedApps() {
  if (!desktop || state.busy) return;
  const button = dialog.querySelector<HTMLButtonElement>('[data-dialog="quit"]')!;
  const status = dialog.querySelector<HTMLElement>('#quit-status')!;
  const acknowledge = dialog.querySelector<HTMLInputElement>('#acknowledge')!;
  state.busy = 'quit'; button.disabled = true; acknowledge.disabled = true; acknowledge.checked = false;
  dialog.querySelector<HTMLButtonElement>('[data-dialog="confirm"]')!.disabled = true;
  status.textContent = 'Requesting apps to quit…'; render();
  try {
    const requested = await invoke<number>('quit_apps', { ids: reviewIds });
    status.textContent = requested
      ? 'Quit requested. Respond to any save prompts, then confirm below once the apps have closed.'
      : 'No selected apps are running. Stop any builds or installations, then confirm below.';
  } catch (error) {
    status.textContent = String(error);
  } finally {
    state.busy = ''; button.disabled = false; acknowledge.disabled = false; render();
  }
}

async function confirmTrash() {
  if (!document.querySelector<HTMLInputElement>('#acknowledge')?.checked || state.busy) return;
  const previousRecords = new Set(state.history.map(item => item.id));
  dialog.close(); state.busy = 'trash'; state.error = ''; render();
  try {
    state.history = await invoke<HistoryItem[]>('trash_items', { ids: reviewIds });
    state.page = 'history';
    notify('Cleanup finished. Review each result in History.');
  } catch (error) {
    state.error = String(error);
    try { const data = await invoke<{ history: HistoryItem[] }>('bootstrap'); state.history = data.history; }
    catch { /* Preserve the original operation error if history is also unavailable. */ }
  } finally {
    const paths = new Set(state.history.filter(item => !previousRecords.has(item.id) && item.status === 'trashed').map(item => item.original));
    state.items = state.items.filter(item => !paths.has(item.path));
    state.selected = new Set(state.items.filter(item => state.selected.has(item.id)).map(item => item.id));
    state.busy = ''; render();
  }
}

async function addRoot() {
  if (!desktop) { notify('Folder selection is available in the desktop app.'); return false; }
  if (state.busy) return false;
  const previous = JSON.stringify(state.settings.roots);
  state.busy = 'folder'; render();
  try {
    state.settings = await invoke<Settings>('choose_root');
    state.permissions = null;
    const changed = JSON.stringify(state.settings.roots) !== previous;
    if (changed) notify('Project folders updated.');
    return changed;
  } finally { state.busy = ''; render(); }
}

app.addEventListener('input', event => { if ((event.target as HTMLElement).id === 'search') { state.query = (event.target as HTMLInputElement).value; render(); } });
app.addEventListener('change', event => {
  const target = event.target as HTMLInputElement;
  if (target.id === 'sort') { state.sort = target.value; render(); return; }
  if (target.dataset.select) {
    const item = state.items.find(i => i.id === target.dataset.select)!;
    if (target.checked && item.category === 'apps' && item.appParent && !state.selected.has(item.appParent)) { target.checked = false; notify('Select the app bundle first, then select its associated data.'); return; }
    selectItem(item, target.checked);
    render();
  }
  if (target.dataset.action === 'select-visible') {
    visibleItems().filter(i => canSelect(i) && (state.page !== 'apps' || !i.appParent)).forEach(item => selectItem(item, target.checked));
    render();
  }
});

app.addEventListener('click', async event => {
  const target = (event.target as HTMLElement).closest<HTMLElement>('button'); if (!target) return;
  try {
    if (target.dataset.page) { state.page = target.dataset.page as Page; state.query = ''; state.detail = null; render(); document.querySelector('.main-scroll')!.scrollTop = 0; return; }
    if (target.dataset.theme) {
      const appearance = target.dataset.theme as Settings['appearance'];
      state.settings = desktop ? await invoke<Settings>('set_appearance', { appearance }) : { ...state.settings, appearance };
      applyAppearance(); render(); return;
    }
    switch (target.dataset.action) {
      case 'check-update': await refreshUpdate(); break;
      case 'update':
        if (desktop && state.updateVersion) { await invoke('open_update'); notify('Opening the DMG download in your browser. Quit Mac Cleaner before replacing it in Applications.'); }
        break;
      case 'scan': await startScan(); break;
      case 'cancel': await invoke('cancel_scan'); state.phase = 'Stopping scan…'; render(); break;
      case 'add-root': { const scanAfter = state.page === 'node'; if (await addRoot() && scanAfter) await startScan(); break; }
      case 'remove-root': state.settings = await invoke<Settings>('remove_root', { index: Number(target.dataset.index) }); state.permissions = null; state.items = []; state.selected.clear(); state.summary = null; render(); break;
      case 'detail': {
        state.detail = target.dataset.id!; render();
        app.querySelector('.details-pane')?.scrollIntoView({ block: 'nearest' });
        app.querySelector<HTMLButtonElement>('[data-action="close-detail"]')?.focus({ preventScroll: true });
        break;
      }
      case 'close-detail': {
        const id = state.detail; state.detail = null; render();
        const row = app.querySelector<HTMLButtonElement>(`[data-action="detail"][data-id="${CSS.escape(id ?? '')}"]`);
        row?.scrollIntoView({ block: 'nearest' }); row?.focus({ preventScroll: true });
        break;
      }
      case 'reveal': if (desktop) await invoke('reveal_item', { id: target.dataset.id }); else notify('Finder is available in the desktop app.'); break;
      case 'open-trash': if (desktop) await invoke('open_trash'); else notify('Trash is available in the desktop app.'); break;
      case 'privacy-settings':
      case 'full-disk-access':
        if (desktop) {
          await invoke('open_privacy_settings', { fullDiskAccess: target.dataset.action === 'full-disk-access' });
          state.recheckPermissions = true; state.permissions = null; state.permissionError = ''; render();
          notify('Approve access in System Settings, then return here to check the result.');
        }
        else notify('macOS permissions are available in the desktop app.');
        break;
      case 'check-permissions': await refreshPermissions(); break;
      case 'review': showReview(); break;
      case 'clear': state.selected.clear(); render(); break;
      case 'dismiss-error': state.error = ''; render(); break;
      case 'restore':
        state.busy = 'restore'; render();
        try { state.history = await invoke<HistoryItem[]>('restore_item', { id: target.dataset.id }); const result = state.history.find(h => h.id === target.dataset.id); notify(result?.status === 'restored' ? 'Item restored to its original location.' : result?.error || 'Could not restore this item.'); }
        finally { state.busy = ''; render(); }
        break;
    }
  } catch (error) { state.error = String(error); render(); }
});
app.addEventListener('mousedown', event => { if (desktop && (event.target as HTMLElement).hasAttribute('data-drag')) void getCurrentWindow().startDragging(); });
dialog.addEventListener('change', () => { const button = dialog.querySelector<HTMLButtonElement>('[data-dialog="confirm"]'); if (button) button.disabled = !!state.busy || !dialog.querySelector<HTMLInputElement>('#acknowledge')?.checked; });
dialog.addEventListener('click', event => { const action = (event.target as HTMLElement).closest<HTMLElement>('button')?.dataset.dialog; if (action === 'close') dialog.close(); if (action === 'quit') void quitSelectedApps(); if (action === 'confirm') void confirmTrash(); });
window.addEventListener('focus', () => {
  if (state.recheckPermissions && !state.busy) { state.recheckPermissions = false; void refreshPermissions(); }
});

async function init() {
  render();
  if (desktop) {
    try { const data = await invoke<{ settings: Settings; history: HistoryItem[] }>('bootstrap'); state.settings = data.settings; state.history = data.history; state.ready = true; }
    catch (error) { state.error = String(error); }
  } else { state.ready = true; }
  applyAppearance(); render();
  if (desktop) void refreshUpdate();
}
void init();
