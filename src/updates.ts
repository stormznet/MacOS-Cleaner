export async function checkUpdate(): Promise<string | null> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10000);
  try {
    const response = await fetch('https://api.github.com/repos/stormznet/MacOS-Cleaner/releases/latest', {
      headers: { Accept: 'application/vnd.github+json' }, signal: controller.signal, cache: 'no-store',
    });
    if (response.status === 404) return null;
    if (!response.ok) throw new Error('Could not check for updates. Try again when you’re online.');
    const release = await response.json();
    const match = typeof release?.tag_name === 'string' && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(release.tag_name);
    if (!match || release.draft || release.prerelease) return null;
    const latest = match.slice(1).map(Number);
    const current = __APP_VERSION__.split('-')[0].split('.').map(Number);
    if (!latest.every(Number.isSafeInteger)) return null;
    const difference = latest.findIndex((part, index) => part !== current[index]);
    if (difference < 0 ? !__APP_VERSION__.includes('-') : latest[difference] < current[difference]) return null;
    const download = `https://github.com/stormznet/MacOS-Cleaner/releases/download/${release.tag_name}/MacCleaner-arm64.dmg`;
    return Array.isArray(release.assets) && release.assets.some((asset: { name?: string; browser_download_url?: string } | null) =>
      asset?.name === 'MacCleaner-arm64.dmg' && asset.browser_download_url === download) ? latest.join('.') : null;
  } finally { clearTimeout(timeout); }
}
