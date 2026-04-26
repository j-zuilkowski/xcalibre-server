const path = require("path");
const { getDefaultConfig } = require("expo/metro-config");

const projectRoot = __dirname;
const monorepoRoot = path.resolve(projectRoot, "../..");

const config = getDefaultConfig(projectRoot);

config.watchFolders = [monorepoRoot];
config.resolver.nodeModulesPaths = [
  path.resolve(projectRoot, "node_modules"),
  path.resolve(monorepoRoot, "node_modules"),
];
config.resolver.resolveRequest = (context, moduleName, platform) => {
  if (moduleName === "expo-router/entry") {
    return {
      type: "sourceFile",
      filePath: path.resolve(projectRoot, "node_modules/expo-router/entry.js"),
    };
  }

  return context.resolveRequest(context, moduleName, platform);
};

module.exports = config;
