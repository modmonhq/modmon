import eslint from "@eslint/js";
import tsParser from "@typescript-eslint/parser";
import vue from "eslint-plugin-vue";

export default [
  { ignores: ["dist/**", "coverage/**", "playwright-report/**", "src/api.validator.generated.*"] },
  eslint.configs.recommended,
  ...vue.configs["flat/recommended"],
  {
    files: ["**/*.mjs"],
    languageOptions: {
      globals: { console: "readonly", process: "readonly", URL: "readonly" },
    },
  },
  {
    files: ["**/*.ts"],
    languageOptions: {
      parser: tsParser,
      parserOptions: { ecmaVersion: "latest", sourceType: "module" },
      globals: {
        document: "readonly",
        Element: "readonly",
        window: "readonly",
        fetch: "readonly",
        EventSource: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        AbortSignal: "readonly",
        AbortController: "readonly",
        DOMException: "readonly",
        MessageEvent: "readonly",
        Response: "readonly",
        URL: "readonly",
        Intl: "readonly",
        process: "readonly",
      },
    },
  },
  {
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: { parser: tsParser, ecmaVersion: "latest", sourceType: "module" },
      globals: {
        document: "readonly",
        window: "readonly",
        fetch: "readonly",
        EventSource: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        AbortSignal: "readonly",
        AbortController: "readonly",
        DOMException: "readonly",
        MessageEvent: "readonly",
        Response: "readonly",
        URL: "readonly",
        Intl: "readonly",
      },
    },
    rules: {
      "vue/multi-word-component-names": "off",
      "vue/max-attributes-per-line": "off",
      "vue/singleline-html-element-content-newline": "off",
      "vue/html-self-closing": "off",
    },
  },
];
