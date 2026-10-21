import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { host: "127.0.0.1", watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "es2022" },
});
