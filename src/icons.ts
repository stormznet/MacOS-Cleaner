const paths: Record<string, string> = {
  overview: '<rect x="3" y="4" width="5" height="16" rx="1.5"/><rect x="10" y="4" width="4" height="16" rx="1.5"/><rect x="16" y="4" width="5" height="16" rx="1.5"/>',
  box: '<path d="m12 3 9 5v9l-9 5-9-5V8l9-5Z"/><path d="m3 8 9 5 9-5M12 13v9M7.5 5.5l9 5"/>',
  tools: '<path d="m14 5 5 5M12 3l-2 2 2 2-8 8a2 2 0 0 0 3 3l8-8 2 2 4-4-5-5-4 0Z"/><path d="m14 16 5 5 2-2-5-5"/>',
  archive: '<rect x="3" y="4" width="18" height="5" rx="1.5"/><path d="M5 9v11h14V9M9 13h6"/>',
  cache: '<rect x="4" y="4" width="16" height="16" rx="4"/><path d="M8 9h8M8 13h5M8 17h3"/>',
  app: '<rect x="3" y="3" width="18" height="18" rx="5"/><path d="m8 8 8 8M16 8l-8 8"/>',
  history: '<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v6l4 2"/>',
  settings: '<path d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z"/><path d="m10 2-1 3-3 1-3-1-1 4 2 2v3l-2 2 2 4 3-1 3 1 1 2 4-1 1-3 3-1 3 1 1-4-2-2v-3l2-2-2-4-3 1-3-1-1-2-4 1Z"/>',
  scan: '<path d="M20 7A9 9 0 1 0 21 14M20 2v5h-5"/>',
  shield: '<path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Z"/><path d="m8 12 3 3 5-6"/>',
  folder: '<path d="M3 7V5h7l2 3h9v12H3V7Z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  arrow: '<path d="m9 5 7 7-7 7"/>',
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>',
  search: '<circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7v1"/>',
};
export function icon(name: string, className = ''): string {
  return `<svg class="icon ${className}" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name] ?? paths.info}</svg>`;
}
