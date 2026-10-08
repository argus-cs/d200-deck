import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri loads the dev server from a fixed port and prints Rust errors to the
// same terminal, so Vite must not clear it.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1430, strictPort: true },
  build: { target: 'es2022', outDir: 'dist' },
});
