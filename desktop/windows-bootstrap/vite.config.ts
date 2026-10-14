import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const directory = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root: resolve(directory, 'frontend'),
  base: './',
  publicDir: false,
  plugins: [react()],
  build: {
    outDir: resolve(directory, 'dist'),
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
    modulePreload: false,
  },
  server: { host: '127.0.0.1', port: 5173, strictPort: true },
});
