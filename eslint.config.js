/**
 * SOURCE OF TRUTH KEYWORDS: eslint config, lint rules, no any, ts-ignore ban, non-null assertion ban, noInlineConfig, type-checked lint
 * WHAT:  Flat ESLint config for every TS/TSX/JS file in the repo (frontend, tools, configs).
 * WHY:   Enforces the rulebook's "no type or error bypass" rule at lint time: `any`, every `@ts-*` directive and
 *        non-null assertions are errors, and `noInlineConfig` makes `eslint-disable` comments inert so a lint can
 *        never be silenced in place. `no-undef` is off because `tsc` (with checkJs) already owns that check.
 *        Generated `src/bindings.ts` is excluded: it is produced by tauri-specta and never edited by hand.
 * WHERE: Run by `pnpm lint` (part of the local gate, 02 §11).
 */
import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default defineConfig(
  globalIgnores(["dist/", "src-tauri/", "src/bindings.ts"]),
  {
    linterOptions: {
      noInlineConfig: true,
      reportUnusedDisableDirectives: "error",
    },
  },
  {
    files: ["**/*.{ts,tsx,js,mjs}"],
    extends: [js.configs.recommended, tseslint.configs.strictTypeChecked, tseslint.configs.stylisticTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "no-undef": "off",
      "@typescript-eslint/no-explicit-any": ["error", { fixToUnknown: false, ignoreRestArgs: false }],
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/ban-ts-comment": [
        "error",
        { "ts-expect-error": true, "ts-ignore": true, "ts-nocheck": true, "ts-check": false },
      ],
      "@typescript-eslint/consistent-type-imports": "error",
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [reactHooks.configs.flat.recommended, reactRefresh.configs.vite],
  },
);
