import preact from '@preact/preset-vite';
import { defineConfig } from 'vitest/config';

// Im Browser (Entwicklung, Tests) läuft der Kern hinter der HTTP-Brücke `dev_bridge`. In der Tauri-App
// ersetzen Tauri-Befehle diese Brücke.
const bridge = process.env.BDB_BRIDGE ?? 'http://127.0.0.1:8787';

export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: {
    port: Number(process.env.BDB_DEV_PORT ?? 5173),
    strictPort: true,
    // Die Bitflip-Tokens und Logos liegen im Ordner `design/` neben `app/`.
    fs: { allow: ['..'] },
    proxy: { '/api': { target: bridge } },
  },
  build: { target: 'es2022' },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
