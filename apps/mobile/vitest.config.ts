import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

export default defineConfig({
  resolve: {
    alias: [
      {
        find: /^react-native$/,
        replacement: fileURLToPath(new URL("./src/test/react-native.tsx", import.meta.url)),
      },
      {
        find: /^react-native\/.+$/,
        replacement: fileURLToPath(new URL("./src/test/react-native.tsx", import.meta.url)),
      },
    ],
  },
  test: {
    environment: "node",
    setupFiles: ["./src/test/setup.ts"],
    globals: true,
  },
});
