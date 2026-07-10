import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 4173,
    proxy: {
      "/console/api": "http://127.0.0.1:8796",
      "/auth": "http://127.0.0.1:8796"
    }
  },
  build: {
    outDir: "dist",
    sourcemap: true
  }
});
