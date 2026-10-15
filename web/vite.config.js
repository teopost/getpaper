import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:3000',
      '/files': 'http://localhost:3000',
      '/auth': 'http://localhost:3000',
      '/mcp': 'http://localhost:3000',
    },
  },
  build: { outDir: 'dist', chunkSizeWarningLimit: 2000 },
});
