import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "build/**", "out/**", "src/generated/**"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,tsx}"],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }]
    }
  },
  {
    extends: [js.configs.recommended],
    files: ["**/*.{js,mjs}"],
    languageOptions: { ecmaVersion: 2022 }
  },
  {
    files: ["src/**/*.js"],
    ignores: ["src/speech/pcm-worklet.js"],
    languageOptions: { globals: globals.browser }
  },
  {
    files: ["src/speech/pcm-worklet.js"],
    languageOptions: { globals: globals.audioWorklet }
  },
  {
    files: ["eslint.config.js"],
    languageOptions: { globals: globals.node }
  },
  {
    files: ["src/**/*.test.ts", "tests/**/*.{ts,tsx,mjs}", "vite.config.ts"],
    languageOptions: { globals: { ...globals.browser, ...globals.node } }
  }
);
