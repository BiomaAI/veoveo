import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "/workspace/",
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
    proxy: {
      "/workspace/api": "http://127.0.0.1:8786",
      "/workspace/auth": "http://127.0.0.1:8786",
      "/console/api": { target: "http://127.0.0.1:8786", ws: true },
      "/auth": "http://127.0.0.1:8786",
      "/oauth": "http://127.0.0.1:8788",
      "/.well-known": "http://127.0.0.1:8788",
    },
  },
  build: { outDir: "dist" },
});
