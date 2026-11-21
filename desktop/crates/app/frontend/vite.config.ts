import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

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

const DEMO = join(import.meta.dirname, 'demo');
const DEMO_TYPES: Record<string, string> = { svg: 'image/svg+xml', mp4: 'video/mp4', mp3: 'audio/mpeg' };
function demoMedia(): Plugin {
  return {
    name: 'demo-media',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const match = req.url?.match(/^\/demo\/([\w-]+)\.(svg|mp4|mp3)(?:\?.*)?$/);
        if (!match) return next();
        try {
          const bytes = readFileSync(join(DEMO, `${match[1]}.${match[2]}`));
          res.setHeader('Content-Type', DEMO_TYPES[match[2]]);
          res.setHeader('Accept-Ranges', 'bytes');
          const range = req.headers.range?.match(/bytes=(\d+)-(\d*)/);
          if (range) {
            const start = Number(range[1]), end = range[2] ? Number(range[2]) : bytes.length - 1;
            res.statusCode = 206;
            res.setHeader('Content-Range', `bytes ${start}-${end}/${bytes.length}`);
            res.end(bytes.subarray(start, end + 1));
          } else {
            res.end(bytes);
          }
        } catch { res.statusCode = 404; res.end(); }
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), pdfjsAssets(), demoMedia()],
  appType: 'spa',
  server: {
    port: 5174,
    strictPort: true,
  },
  clearScreen: false,
})
