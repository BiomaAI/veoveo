import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "/console/",
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 4173,
    strictPort: true,
    proxy: {
      "/console/api": { target: "http://127.0.0.1:8786", ws: true },
      "/auth": "http://127.0.0.1:8786",
      "/oauth": "http://127.0.0.1:8788",
      "/.well-known": "http://127.0.0.1:8788"
    }
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    rollupOptions: {
      input: {
        console: "index.html",
        appHost: "app-host.html"
      }
    }
  }
});
