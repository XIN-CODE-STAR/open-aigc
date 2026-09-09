import eslint from "@eslint/js";
import eslintConfigPrettier from "eslint-config-prettier";
import eslintPluginVue from "eslint-plugin-vue";
import globals from "globals";
import typescriptEslint from "typescript-eslint";

export default typescriptEslint.config(
  {
    ignores: [
      "**/*.d.ts",
      "coverage/**",
      "dist/**",
      "node_modules/**",
      ".trae/**",
      "src-tauri/gen/**",
      "src-tauri/target/**",
      "work/**",
      // jimeng-api 代理子项目有自己的工具链，构建产物与源码均不由本仓库 lint。
      "services/jimeng-api/**",
    ],
  },
  eslint.configs.recommended,
  ...typescriptEslint.configs.strict,
  ...typescriptEslint.configs.stylistic,
  ...eslintPluginVue.configs["flat/recommended-error"],
  {
    files: ["**/*.{ts,vue}"],
    languageOptions: {
      ecmaVersion: "latest",
      globals: globals.browser,
      parserOptions: {
        parser: typescriptEslint.parser,
      },
      sourceType: "module",
    },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-import-type-side-effects": "error",
      "vue/multi-word-component-names": ["error", { ignores: ["App"] }],
    },
  },
  {
    files: ["**/*.{js,mjs,cjs}"],
    languageOptions: {
      globals: globals.node,
    },
  },
  eslintConfigPrettier,
);
