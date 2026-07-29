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
      // The paraglide-js Vite plugin's *implementation* output (see
      // .gitignore) — plain `.js`, so typescript-eslint's project service
      // has nowhere to type-check them from and reports a parse error.
      // Its sibling `.d.ts` files are what every import actually resolves
      // types against, so those stay in the project (see the dedicated
      // block below) rather than being blanket-ignored here, which would
      // otherwise break type resolution for every file that imports them.
      "src/paraglide/**/*.js",
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
  {
    // The paraglide-js Vite plugin's generated `.d.ts` declarations (see
    // .gitignore and the `ignores` entry above for its `.js` counterparts)
    // — deliberately kept inside typescript-eslint's project, just exempt
    // from style rules that don't apply to generated code.
    files: ["src/paraglide/**/*.d.ts"],
    rules: {
      "@typescript-eslint/no-empty-object-type": "off",
      "@typescript-eslint/consistent-type-definitions": "off",
      "@typescript-eslint/array-type": "off",
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
);
