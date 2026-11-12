// Browser preview of the UI without the Tauri backend: the normal app, served with the
// sample data in mock.ts. Run: node node_modules/vite/bin/vite.js --config .preview/vite.config.ts
import { fileURLToPath, URL } from "node:url";
import { defineConfig, mergeConfig } from "vite";
import base from "../vite.config";

export default mergeConfig(
  base,
  defineConfig({
    root: fileURLToPath(new URL("..", import.meta.url)),
    // 1420 belongs to `tauri dev`.
    server: { port: 1421 },
    plugins: [
      {
        name: "veyl-sample-data",
        // First in <head>, so it installs the fake backend before the app's own module runs.
        transformIndexHtml: () => [{ tag: "script", attrs: { type: "module", src: "/.preview/mock.ts" }, injectTo: "head-prepend" }],
      },
    ],
  }),
);
