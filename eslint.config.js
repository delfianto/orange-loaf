// @ts-check
import { fileURLToPath } from "node:url";
import { defineConfig } from "eslint/config";
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import svelte from "eslint-plugin-svelte";
import prettier from "eslint-config-prettier";
import globals from "globals";

const tsconfigRootDir = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig(
  {
    // eslint.config.js itself is intentionally not linted: it's the one file that can
    // never be covered by its own project's tsconfig (bootstrapping paradox), and it's
    // trivial, low-risk, human-reviewed tooling config rather than application code.
    ignores: [
      "dist/**",
      "src-tauri/**",
      "node_modules/**",
      "public/**",
      ".superdesign/**",
      "eslint.config.js",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  ...svelte.configs.recommended,
  prettier,
  ...svelte.configs.prettier,
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
      parserOptions: {
        projectService: true,
        extraFileExtensions: [".svelte"],
        tsconfigRootDir,
      },
    },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
);
