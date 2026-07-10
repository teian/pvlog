import js from "@eslint/js";
import boundaries from "eslint-plugin-boundaries";
import jsxA11y from "eslint-plugin-jsx-a11y";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import unicorn from "eslint-plugin-unicorn";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "tests/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  react.configs.flat.recommended,
  react.configs.flat["jsx-runtime"],
  reactHooks.configs.flat.recommended,
  jsxA11y.flatConfigs.strict,
  {
    files: ["src/ui/**/*.{ts,tsx}"],
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      boundaries,
      unicorn,
    },
    settings: {
      react: { version: "detect" },
      "boundaries/elements": [
        { type: "app", pattern: "src/ui/app/*" },
        { type: "pages", pattern: "src/ui/pages/*" },
        { type: "widgets", pattern: "src/ui/widgets/*" },
        { type: "features", pattern: "src/ui/features/*" },
        { type: "entities", pattern: "src/ui/entities/*" },
        { type: "shared", pattern: "src/ui/shared/*" },
      ],
    },
    rules: {
      "boundaries/dependencies": [
        "error",
        {
          default: "disallow",
          policies: [
            {
              from: { element: { types: "app" } },
              allow: {
                to: {
                  element: {
                    types: {
                      anyOf: [
                        "pages",
                        "widgets",
                        "features",
                        "entities",
                        "shared",
                      ],
                    },
                  },
                },
              },
            },
            {
              from: { element: { types: "pages" } },
              allow: {
                to: {
                  element: {
                    types: {
                      anyOf: ["widgets", "features", "entities", "shared"],
                    },
                  },
                },
              },
            },
            {
              from: { element: { types: "widgets" } },
              allow: {
                to: {
                  element: {
                    types: { anyOf: ["features", "entities", "shared"] },
                  },
                },
              },
            },
            {
              from: { element: { types: "features" } },
              allow: {
                to: { element: { types: { anyOf: ["entities", "shared"] } } },
              },
            },
            {
              from: { element: { types: "entities" } },
              allow: { to: { element: { types: "shared" } } },
            },
            {
              from: { element: { types: "shared" } },
              allow: { to: { element: { types: "shared" } } },
            },
          ],
        },
      ],
      "react/no-multi-comp": "error",
      "react/prop-types": "off",
      "react/jsx-no-literals": "warn",
      "unicorn/filename-case": [
        "error",
        { cases: { camelCase: true, pascalCase: true } },
      ],
      complexity: ["warn", 10],
      "max-lines": [
        "warn",
        { max: 300, skipBlankLines: true, skipComments: true },
      ],
      "max-lines-per-function": [
        "warn",
        { max: 100, skipBlankLines: true, skipComments: true },
      ],
      "@typescript-eslint/consistent-type-imports": "error",
    },
  },
  {
    files: ["*.config.{js,ts}"],
    languageOptions: {
      globals: globals.node,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
);
