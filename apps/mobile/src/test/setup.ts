import { vi } from "vitest";

vi.mock("expo-image", () => {
  return {
    Image: "Image",
  };
});
