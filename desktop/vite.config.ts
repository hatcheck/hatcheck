import { resolve } from "node:path";
import { defineConfig } from "vite";

// Two pages: the menu-bar popover (tray.html) and the main window (index.html).
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    rollupOptions: {
      input: { main: resolve(__dirname, "index.html"), tray: resolve(__dirname, "tray.html") },
    },
  },
});
