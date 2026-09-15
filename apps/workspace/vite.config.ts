import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

export default defineConfig({
  base: "/workspace/",
  plugins: [react()],
  resolve: {
    dedupe: ["react", "react-dom", "@tanstack/react-query", "zod", "lucide-react"],
    alias: Object.fromEntries(["react", "react-dom", "@tanstack/react-query", "zod", "lucide-react"].map(name =>
      [name, fileURLToPath(new URL(`./node_modules/${name}`, import.meta.url))])),
  },
  server: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
    fs: { allow: [fileURLToPath(new URL("../", import.meta.url))] },
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
