const fs = require("node:fs");
const path = require("node:path");

const sharedBaseConfigPath = path.resolve(
  __dirname,
  "../../packages/shared/tailwind.config.base.js",
);

const sharedBase = fs.existsSync(sharedBaseConfigPath)
  ? require(sharedBaseConfigPath)
  : {};

module.exports = {
  content: ["./app/**/*.tsx", "./src/app/**/*.tsx", "./src/components/**/*.tsx"],
  theme: {
    extend: {
      ...(sharedBase.theme?.extend ?? {}),
      colors: {
        ...(sharedBase.theme?.extend?.colors ?? {}),
        zinc: {
          50: "#fafafa",
          200: "#e4e4e7",
          500: "#71717a",
          900: "#18181b",
        },
        teal: {
          600: "#0f766e",
        },
      },
    },
  },
  plugins: [],
};
