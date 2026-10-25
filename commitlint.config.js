export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "header-max-length": [2, "always", 120],
    "subject-full-stop": [2, "never", "."],
    "subject-case": [2, "always", "lower-case"],
  },
};
