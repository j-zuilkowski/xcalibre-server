const appJson = require("./app.json");

module.exports = () => {
  const config = appJson.expo ?? {};
  const plugins = Array.isArray(config.plugins) ? [...config.plugins] : [];

  if (process.env.EXPO_DISABLE_SQLITE_CONFIG_PLUGIN === "1") {
    return {
      ...config,
      plugins: plugins.filter((plugin) => plugin !== "expo-sqlite"),
    };
  }

  return {
    ...config,
    plugins,
  };
};
