import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, webkit, expect } from '@playwright/test';
import { createServer } from 'vite';
import { mkdir } from 'node:fs/promises';

let server, browser;
before(async () => {
  server = await createServer({ server: { host: '127.0.0.1', port: 1427, strictPort: true } });
  await server.listen();
  browser = await (process.env.UI_ENGINE === 'webkit' ? webkit : chromium).launch();
  await mkdir('test-results', { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

test('scan progress covers discovery, percentages, errors, cancellation, and an empty scan', async () => {
  const page = await browser.newPage({ viewport: { width: 1180, height: 800 } });
  await page.addInitScript(() => {
    window.isTauri = true;
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1, unregisterCallback: () => {},
      invoke: async (command, args) => {
        if (command === 'bootstrap') return { settings: { roots: ['/Users/test/projects'], appearance: 'dark' }, history: [] };
        if (command === 'scan') {
          window.emitScan = event => args.onEvent.onmessage(event);
          return new Promise((resolve, reject) => { window.finishScan = resolve; window.failScan = reject; });
        }
        if (command === 'cancel_scan') { window.finishScan({ count: 4, elapsedMs: 100, cancelled: true, warnings: [] }); return; }
        throw new Error(`Unexpected command: ${command}`);
      },
    };
  });
  await page.goto('http://127.0.0.1:1427');
  await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
  const bar = page.getByRole('progressbar', { name: 'Scan progress' });
  await expect(bar).toBeVisible();
  await expect(bar).not.toHaveAttribute('value');
  await expect(page.locator('.scan-progress')).toContainText('Finding items');
  await page.evaluate(() => window.emitScan({ kind: 'progress', processed: 0, total: 4 }));
  await expect(bar).toHaveAttribute('value', '0');
  await page.evaluate(() => window.emitScan({ kind: 'progress', processed: 1, total: 4 }));
  await expect(bar).toHaveAttribute('value', '25');
  await page.evaluate(() => { window.emitScan({ kind: 'progress', processed: 2, total: 4 }); window.emitScan({ kind: 'progress', processed: 1, total: 4 }); });
  await expect(bar).toHaveAttribute('value', '50');
  await expect(page.locator('.scan-progress')).toContainText('2 of 4 items checked');
  await page.locator('nav [data-page=settings]').click();
  await expect(bar).toHaveAttribute('value', '50');
  await page.screenshot({ path: 'test-results/scan-progress-fixtures.png' });
  await page.evaluate(() => window.emitScan({ kind: 'progress', processed: 4, total: 4 }));
  await expect(bar).toHaveAttribute('value', '99');
  await page.evaluate(() => window.finishScan({ count: 4, elapsedMs: 100, cancelled: false, warnings: ['Fixture access notice'] }));
  await expect(bar).toHaveAttribute('value', '100');
  await expect(page.locator('.scan-progress')).toContainText('Scan complete with notices');
  await page.getByRole('button', { name: 'Scan again', exact: true }).click();
  await expect(bar).not.toHaveAttribute('value');
  await page.evaluate(() => window.emitScan({ kind: 'progress', processed: 0, total: 0 }));
  await expect(bar).toHaveAttribute('value', '0');
  await page.evaluate(() => window.finishScan({ count: 0, elapsedMs: 100, cancelled: false, warnings: [] }));
  await expect(bar).toHaveAttribute('value', '100');
  await page.getByRole('button', { name: 'Scan again', exact: true }).click();
  await page.evaluate(() => window.emitScan({ kind: 'progress', processed: 1, total: 4 }));
  await expect(bar).toHaveAttribute('value', '25');
  await page.getByRole('button', { name: 'Stop scan', exact: true }).click();
  await expect(page.locator('.scan-progress')).toContainText('Scan stopped');
  await expect(bar).toHaveAttribute('value', '25');
  await page.getByRole('button', { name: 'Scan again', exact: true }).click();
  await page.evaluate(() => window.failScan('Fixture scan failed'));
  await expect(page.getByRole('alert')).toContainText('Fixture scan failed');
  await expect(bar).toHaveCount(0);
  await page.close();
});

test('details stay visible for lower rows and preserve reading position across renders', async () => {
  for (const width of [1180, 900, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 800 }, colorScheme: 'dark' });
    await page.addInitScript(() => {
      window.isTauri = true;
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 1, unregisterCallback: () => {},
        invoke: async (command, args) => {
          if (command === 'bootstrap') return { settings: { roots: ['/Users/test/projects'], appearance: 'dark' }, history: [] };
          if (command === 'scan') {
            for (let i = 0; i < 40; i++) args.onEvent.onmessage({ kind: 'item', item: {
              id: `project-${i}`, name: `Fixture project ${i}`, category: 'node', root: '/Users/test/projects',
              path: `/Users/test/projects/project-${i}/node_modules`, bytes: 4096, modified: 1750000000,
              description: 'Long description for scrolling verification. '.repeat(80), consequence: 'Reinstall required',
              complete: true, selectable: true, issue: null, bundleId: null, appParent: null, version: null,
            } });
            return { count: 40, elapsedMs: 100, cancelled: false, warnings: [] };
          }
          throw new Error(`Unexpected command: ${command}`);
        },
      };
    });
    await page.goto('http://127.0.0.1:1427');
    await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
    await page.locator('nav [data-page=node]').click();
    const row = page.locator('[data-action=detail][data-id="project-25"]');
    await row.click();
    const pane = page.getByRole('complementary', { name: 'Item details' });
    await expect(pane.getByRole('heading', { name: 'Fixture project 25', exact: true })).toBeInViewport();
    await expect(page.getByRole('button', { name: 'Close details' })).toBeInViewport();
    if (width > 700) {
      await page.locator('.main-scroll').evaluate(el => { el.scrollTop += 150; });
      await expect(pane.getByRole('heading', { name: 'Fixture project 25', exact: true })).toBeInViewport();
      await pane.evaluate(el => { el.scrollTop = 200; });
      await page.getByRole('checkbox', { name: 'Select Fixture project 25', exact: true }).check();
      assert.equal(await pane.evaluate(el => el.scrollTop), 200);
    }
    await pane.evaluate(el => { el.scrollTop = el.scrollHeight; });
    await expect(pane.getByText('Modified', { exact: true })).toBeInViewport();
    await pane.evaluate(el => { el.scrollTop = 0; });
    await page.screenshot({ path: `test-results/details-scrolled-${width}.png` });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.getByRole('button', { name: 'Close details' }).click();
    await expect(pane).toHaveCount(0);
    await expect(row).toBeFocused();
    await expect(row).toBeInViewport();
    await page.close();
  }
});

test('browser preview: honest empty state, navigation, appearance, and narrow layout', async () => {
  const page = await browser.newPage({ viewport: { width: 1180, height: 800 }, colorScheme: 'dark' });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('http://127.0.0.1:1427');
  await expect(page.getByRole('heading', { name: 'Make room for what’s next.' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Review selection', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('no filesystem access');
  await page.screenshot({ path: 'test-results/overview-dark.png' });
  for (const category of ['node', 'xcode', 'packages', 'caches', 'apps', 'history', 'settings']) {
    await page.locator(`nav [data-page="${category}"]`).click();
    await expect(page.locator(`nav [data-page="${category}"]`)).toHaveAttribute('aria-current', 'page');
  }
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: 'test-results/settings-light.png' });
  await page.getByRole('button', { name: 'Dark', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'System', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'system');
  await expect(page.getByRole('heading', { name: 'Permissions', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Open Full Disk Access', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('permissions are available in the desktop app');
  await page.setViewportSize({ width: 390, height: 844 });
  for (const category of ['overview', 'node', 'history', 'settings']) {
    await page.locator(`nav [data-page="${category}"]`).click();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  }
  assert.deepEqual(errors, []);
  await page.close();
});

test('first scan chooses project scope, handles cancellation and failure, and opens permission settings', async () => {
  for (const mode of ['choose', 'cancel', 'error']) {
    const page = await browser.newPage({ viewport: { width: 1180, height: 800 } });
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(pickMode => {
      window.isTauri = true;
      window.pickMode = pickMode;
      window.commands = [];
      let settings = { appearance: 'system', roots: [] };
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 1, unregisterCallback: () => {},
        invoke: async (command, args) => {
          window.commands.push({ command, fullDiskAccess: args?.fullDiskAccess });
          switch (command) {
            case 'bootstrap': return { settings, history: [] };
            case 'choose_root':
              if (window.pickMode === 'error') throw new Error('Fixture project folder is inaccessible');
              if (window.pickMode === 'choose') settings = { ...settings, roots: ['/Users/test/projects'] };
              return settings;
            case 'scan':
              if (settings.roots.length) args.onEvent.onmessage({ kind: 'item', item: {
                id: 'dependencies', name: 'Fixture project', category: 'node', path: '/Users/test/projects/web/node_modules',
                root: settings.roots[0], bytes: 4096, modified: 1750000000, description: 'Project dependencies',
                consequence: 'Reinstall required', complete: true, selectable: true, issue: null,
                bundleId: null, appParent: null, version: null,
              } });
              return { count: settings.roots.length, elapsedMs: 100, cancelled: false,
                warnings: settings.roots.length ? [] : ['node_modules was not scanned: add a project folder in Settings or node_modules, then scan again.'] };
            case 'open_privacy_settings': return null;
            default: throw new Error(`Unexpected fixture command: ${command}`);
          }
        },
      };
    }, mode);
    await page.goto('http://127.0.0.1:1427');
    await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
    if (mode === 'error') {
      await expect(page.getByRole('alert')).toContainText('Fixture project folder is inaccessible');
      assert.deepEqual(await page.evaluate(() => window.commands.map(c => c.command)), ['bootstrap', 'choose_root']);
      await expect(page.getByRole('button', { name: 'Scan my Mac', exact: true })).toBeEnabled();
      await page.evaluate(() => { window.pickMode = 'choose'; });
      await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
    } else {
      assert.deepEqual(await page.evaluate(() => window.commands.map(c => c.command)), ['bootstrap', 'choose_root', 'scan']);
    }
    if (mode === 'cancel') {
      await expect(page.locator('.category-card.node')).toContainText('Not scanned');
      await page.locator('nav [data-page=node]').click();
      await expect(page.getByRole('heading', { name: 'Choose a project folder', exact: true })).toBeVisible();
      await page.evaluate(() => { window.pickMode = 'choose'; });
      await page.getByRole('button', { name: 'Add folder & scan', exact: true }).click();
    } else {
      await page.locator('nav [data-page=node]').click();
    }
    await expect(page.locator('.item-row')).toContainText('/Users/test/projects/web/node_modules');
    await expect(page.getByRole('checkbox', { name: 'Select Fixture project', exact: true })).toBeEnabled();
    await expect(page.locator('.selection-footer')).toContainText('0 items selected');
    const picks = await page.evaluate(() => window.commands.filter(c => c.command === 'choose_root').length);
    await page.getByRole('button', { name: 'Scan again', exact: true }).click();
    assert.equal(await page.evaluate(() => window.commands.filter(c => c.command === 'choose_root').length), picks);
    await page.locator('nav [data-page=settings]').click();
    await page.getByRole('button', { name: 'Open Privacy & Security', exact: true }).click();
    await page.getByRole('button', { name: 'Open Full Disk Access', exact: true }).click();
    assert.deepEqual(await page.evaluate(() => window.commands.filter(c => c.command === 'open_privacy_settings').map(c => c.fullDiskAccess)), [false, true]);
    assert.equal(await page.evaluate(() => window.commands.some(c => c.command === 'trash_items')), false);
    if (mode === 'choose') await page.screenshot({ path: 'test-results/permissions-fixtures.png', fullPage: true });
    assert.deepEqual(errors, []);
    await page.close();
  }
});

test('selection, app association, review confirmation, failures, and restore through mocked IPC', async () => {
  const page = await browser.newPage({ viewport: { width: 1180, height: 800 }, colorScheme: 'dark' });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    window.isTauri = true;
    window.commands = [];
    const fixture = (id, category, name, size, extra = {}) => ({ id, category, name, path: `/Users/test/fixtures/${id}`, root: '/Users/test/fixtures', bytes: size,
      modified: 1750000000, description: 'Automated test fixture', consequence: 'Fixture removal consequence', complete: true,
      selectable: true, issue: null, bundleId: null, appParent: null, version: null, ...extra });
    const items = [fixture('project-a', 'node', 'Fixture project A', 1950000000), fixture('project-b', 'node', 'Fixture project B', 500000000),
      fixture('derived', 'xcode', 'Fixture DerivedData', 1750000000), fixture('npm', 'packages', 'Fixture npm cache', 1560000000),
      fixture('cache', 'caches', 'Fixture app cache', 230000000), fixture('unknown', 'caches', '<img src=x onerror=alert(1)>', 100, { selectable: false, issue: 'Owner not recognized' }),
      fixture('app', 'apps', 'Fixture App', 800000000), fixture('preferences', 'apps', 'Fixture preferences', 10000, { appParent: 'app' })];
    let history = [];
    let settings = { appearance: 'dark', roots: ['/Users/test/fixtures'] };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1, unregisterCallback: () => {},
      invoke: async (command, args) => {
        window.commands.push({ command, ids: args?.ids });
        switch (command) {
          case 'bootstrap': return { settings, history };
          case 'scan':
            for (const item of items) args.onEvent.onmessage({ kind: 'item', item });
            return { count: items.length, elapsedMs: 1200, cancelled: false, warnings: ['Fixture permission notice'] };
          case 'trash_items':
            history = args.ids.map(id => { const item = items.find(i => i.id === id); return { id: `record-${id}`, operation: 'test', name: item.name, original: item.path, bytes: item.bytes, timestamp: 1750000000,
              trashPath: `/Users/test/.Trash/${id}`, status: id === 'app' ? 'failed' : 'trashed', error: id === 'app' ? 'Quit the owning app' : null }; });
            return history;
          case 'restore_item': history = history.map(item => item.id === args.id ? { ...item, status: 'restored' } : item); return history;
          case 'set_appearance': settings = { ...settings, appearance: args.appearance }; return settings;
          case 'choose_root': settings = { ...settings, roots: [...settings.roots, '/Users/test/another-project'] }; return settings;
          case 'remove_root': settings.roots.splice(args.index, 1); return settings;
          default: return null;
        }
      },
    };
  });
  await page.goto('http://127.0.0.1:1427');
  await page.getByRole('button', { name: 'Scan my Mac', exact: true }).click();
  await expect(page.locator('.selection-footer')).toContainText('0 items selected');
  await page.screenshot({ path: 'test-results/overview-fixtures.png' });
  await page.locator('nav [data-page=node]').click();
  await page.getByRole('checkbox', { name: 'Select Fixture project A', exact: true }).focus();
  await page.keyboard.press('Space');
  await expect(page.locator('.selection-footer')).toContainText('1 item selected');
  await expect(page.getByRole('checkbox', { name: 'Select Fixture project A', exact: true })).toBeFocused();
  await page.getByRole('searchbox').fill('project B');
  await expect(page.locator('.item-row')).toHaveCount(1);
  await expect(page.locator('.selection-footer')).toContainText('1 item selected');
  await page.getByRole('searchbox').fill('');
  await page.getByRole('combobox', { name: 'Sort items' }).selectOption('name');
  await page.getByRole('checkbox', { name: 'Select 2 visible items', exact: true }).check();
  await expect(page.locator('.selection-footer')).toContainText('2 items selected');
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(page.locator('.selection-footer')).toContainText('0 items selected');
  await page.getByRole('checkbox', { name: 'Select Fixture project A', exact: true }).check();
  await page.locator('[data-action=detail]').first().click();
  await expect(page.getByRole('complementary', { name: 'Item details' })).toContainText('Automated test fixture');
  await page.getByRole('button', { name: 'Show in Finder' }).click();
  await page.screenshot({ path: 'test-results/results-fixtures.png' });
  await page.getByRole('button', { name: 'Close details' }).click();
  await page.getByRole('button', { name: 'Review 1.95 GB' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Move 1 to Trash', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  assert.equal(await page.evaluate(() => window.commands.some(c => c.command === 'trash_items')), false);
  await page.getByRole('button', { name: 'Review 1.95 GB' }).click();
  await page.getByRole('checkbox', { name: 'I’ve quit affected apps and stopped builds or installations.' }).check();
  await page.getByRole('button', { name: 'Move 1 to Trash', exact: true }).click();
  await expect(page.locator('.history-row')).toHaveCount(1);
  assert.deepEqual(await page.evaluate(() => window.commands.find(c => c.command === 'trash_items').ids), ['project-a']);
  await page.getByRole('button', { name: 'Restore', exact: true }).click();
  await expect(page.locator('.history-row')).toContainText('Restored');
  await page.locator('nav [data-page=apps]').click();
  await page.getByRole('checkbox', { name: 'Select Fixture preferences', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Select the app bundle first');
  await expect(page.getByRole('checkbox', { name: 'Select Fixture preferences', exact: true })).not.toBeChecked();
  await page.getByRole('checkbox', { name: 'Select Fixture App', exact: true }).check();
  await page.getByRole('checkbox', { name: 'Select Fixture preferences', exact: true }).check();
  await expect(page.locator('.selection-footer')).toContainText('2 items selected');
  await page.getByRole('checkbox', { name: 'Select Fixture App', exact: true }).uncheck();
  await expect(page.locator('.selection-footer')).toContainText('0 items selected');
  await page.getByRole('checkbox', { name: 'Select Fixture App', exact: true }).check();
  await page.getByRole('button', { name: 'Review 800 MB' }).click();
  await page.getByRole('checkbox', { name: 'I’ve quit affected apps and stopped builds or installations.' }).check();
  await page.getByRole('button', { name: 'Move 1 to Trash', exact: true }).click();
  await expect(page.locator('.history-row')).toContainText('Not moved');
  await expect(page.locator('.history-row')).toContainText('Quit the owning app');
  await expect(page.getByRole('button', { name: 'Restore', exact: true })).toHaveCount(0);
  await page.locator('nav [data-page=caches]').click();
  await expect(page.locator('.item-row input:disabled')).toHaveCount(1);
  await expect(page.locator('.item-row img')).toHaveCount(0);
  await page.locator('nav [data-page=settings]').click();
  await page.getByRole('button', { name: 'Add folder', exact: true }).click();
  await expect(page.locator('.root-list')).toContainText('another-project');
  await page.getByRole('button', { name: 'Remove /Users/test/another-project from scan folders' }).click();
  await expect(page.locator('.root-list')).not.toContainText('another-project');
  assert.deepEqual(errors, []);
  await page.close();
});
