import path from 'node:path';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

const host = process.env.TAURI_DEV_HOST || '127.0.0.1';

export default defineConfig({
  base: './',
  clearScreen: false,
  plugins: [vue(), tailwindcss()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) {
            return;
          }

          if (id.includes('markstream-vue')) {
            return 'vendor-markstream';
          }

          if (id.includes('@tauri-apps')) {
            return 'vendor-tauri';
          }

          if (id.includes('reka-ui')) {
            return 'vendor-reka';
          }

          if (id.includes('@iconify') || id.includes('lucide-vue-next')) {
            return 'vendor-icons';
          }

          if (id.includes('@vue') || id.includes('/vue/')) {
            return 'vendor-vue';
          }

          return 'vendor-misc';
        },
      },
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    host,
    port: 5173,
    strictPort: true,
    warmup: {
      clientFiles: [
        './src/**/*.vue',
        './src/**/*.ts',
        './src/**/*.css',
      ],
    },
  },
  preview: {
    host,
    port: 5173,
    strictPort: true,
  },
});
