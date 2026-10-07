// Only the hooks rules. `tsc` already covers what a general lint would, and
// this codebase leans on refs and effect dependencies being exactly right:
// most of the bugs in docs/reviews/ are a callback reading state it should
// not have closed over. Two `eslint-disable` comments in App.tsx named this
// rule long before anything ran it.
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default [
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: { parser: tseslint.parser },
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
];
