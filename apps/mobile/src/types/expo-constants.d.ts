declare module "expo-constants" {
  type ExpoConfig = {
    version?: string;
  };

  const Constants: {
    expoConfig?: ExpoConfig | null;
  };

  export default Constants;
}
