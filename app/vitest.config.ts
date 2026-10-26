import { defineConfig, mergeConfig } from "vitest/config";
import { svelteTesting } from "@testing-library/svelte/vite";
import viteConfig from "./vite.config";

export default mergeConfig(
  viteConfig,
  defineConfig({
    plugins: [svelteTesting()],
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.test.ts"],
      // Process CSS instead of stubbing it, so `?raw` imports return the real file text.
      css: true,
    },
  }),
);
