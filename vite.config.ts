import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

export default defineConfig({
  server: { port: 1420 },
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          // Split large vendor deps into separate chunks
          "vendor-react": ["react", "react-dom"],
          "vendor-i18n": ["i18next", "react-i18next", "i18next-browser-languagedetector"],
          "vendor-ui": ["radix-ui", "cmdk", "class-variance-authority", "clsx", "tailwind-merge"],
          "vendor-icons": ["lucide-react"],
          "vendor-diff": ["react-diff-viewer-continued"],
        },
      },
    },
    // Target smaller chunks
    chunkSizeWarningLimit: 300,
  },
});
