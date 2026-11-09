import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

export default defineConfig({
  base: process.env.SITE_BASE || "/NekoCrate-Launcher/",
  plugins: [vue()],
  resolve: {
    // The shared launcher backdrop must use the site's Vue instance in CI,
    // where the desktop application's dependencies are intentionally absent.
    alias: {
      vue: fileURLToPath(
        new URL(
          "./node_modules/vue/dist/vue.runtime.esm-bundler.js",
          import.meta.url,
        ),
      ),
    },
    dedupe: ["vue"],
  },
  build: {
    rollupOptions: {
      input: {
        download: fileURLToPath(new URL("./index.html", import.meta.url)),
        features: fileURLToPath(
          new URL("./features/index.html", import.meta.url),
        ),
        docs: fileURLToPath(new URL("./docs/index.html", import.meta.url)),
      },
    },
  },
});
