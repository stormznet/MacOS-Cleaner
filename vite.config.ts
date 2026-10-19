import { defineConfig } from 'vite';
import tauriConfig from './src-tauri/tauri.conf.json' with { type: 'json' };

export default defineConfig({
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(tauriConfig.version) },
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: { target: 'safari16' },
});
