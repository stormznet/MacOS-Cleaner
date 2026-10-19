export interface Item {
  id: string; name: string; path: string; root: string; category: string;
  bytes: number; modified: number; description: string; consequence: string;
  complete: boolean; selectable: boolean; issue: string | null;
  bundleId: string | null; appParent: string | null; version: string | null;
}
export interface HistoryItem {
  id: string; operation: string; name: string; original: string; trashPath: string | null;
  bytes: number; timestamp: number; status: string; error: string | null;
}
export interface Settings { roots: string[]; appearance: 'system' | 'dark' | 'light' }
export interface AccessCheck { name: string; path: string; status: 'success' | 'failed' | 'unavailable'; message: string }
export interface Summary { elapsedMs: number; cancelled: boolean; count: number; warnings: string[] }
export interface ScanProgress { processed: number; total: number }
export type ScanEvent = { kind: 'item'; item: Item } | { kind: 'phase' | 'warning'; message: string } | ({ kind: 'progress' } & ScanProgress);

export const categories = {
  node: { name: 'node_modules', short: 'Dependencies', icon: 'box', description: 'Project dependencies. Reinstall when you need them.', consequence: 'Reinstall required' },
  xcode: { name: 'Xcode', short: 'Xcode build data', icon: 'tools', description: 'Build products and indexes that Xcode creates.', consequence: 'Rebuild required' },
  packages: { name: 'Package caches', short: 'Package caches', icon: 'archive', description: 'Downloads your package managers can fetch again.', consequence: 'Download required' },
  caches: { name: 'App caches', short: 'App caches', icon: 'cache', description: 'Temporary app data. Review the owner before removing.', consequence: 'Review the app' },
  apps: { name: 'Applications', short: 'Applications', icon: 'app', description: 'Uninstall apps with their detected associated files.', consequence: 'Uninstall an app' },
} as const;
export type Category = keyof typeof categories;
export type Page = Category | 'overview' | 'history' | 'settings';

export function bytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const exponent = Math.min(Math.floor(Math.log10(value) / 3), units.length - 1);
  return `${new Intl.NumberFormat('en', { maximumFractionDigits: exponent >= 3 ? 2 : 1 }).format(value / 1000 ** exponent)} ${units[exponent]}`;
}
export function escape(value: unknown): string {
  return String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}
export function selectedItems(items: Item[], selected: Set<string>): Item[] {
  return items.filter(item => selected.has(item.id) && item.complete && item.selectable);
}
export function storageTotal(items: Item[]): number { return items.reduce((total, item) => total + item.bytes, 0); }
