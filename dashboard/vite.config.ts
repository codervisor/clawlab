import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

const API_PROXY_TARGET =
  process.env.CLAWDEN_API_URL || 'http://localhost:8080';

export default defineConfig({
  plugins: [tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: API_PROXY_TARGET,
        changeOrigin: true,
        ws: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
});
