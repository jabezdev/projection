import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// pdf.js (PDF import) loads its standard fonts, character maps and image
// decoders at run time; serve them from /pdfjs/ in dev and copy them into
// the build.
const PDFJS = join(import.meta.dirname, 'node_modules/pdfjs-dist');
const PDFJS_FOLDERS = ['standard_fonts', 'cmaps', 'wasm'];
function pdfjsAssets(): Plugin {
  return {
    name: 'pdfjs-assets',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const match = req.url?.match(/^\/pdfjs\/([a-z_]+)\/([\w.-]+)$/);
        if (!match || !PDFJS_FOLDERS.includes(match[1])) return next();
        try { res.end(readFileSync(join(PDFJS, match[1], match[2]))); } catch { res.statusCode = 404; res.end(); }
      });
    },
    generateBundle() {
      for (const folder of PDFJS_FOLDERS) {
        for (const file of readdirSync(join(PDFJS, folder))) {
          this.emitFile({ type: 'asset', fileName: `pdfjs/${folder}/${file}`, source: readFileSync(join(PDFJS, folder, file)) });
        }
      }
    },
  };
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss(), pdfjsAssets()],
  appType: 'spa',
  // Tauri expects a fixed port, fail if it's already in use
  server: {
    port: 5174,
    strictPort: true,
  },
  // Tries to prevent Vite from obscuring Rust errors
  clearScreen: false,
})
