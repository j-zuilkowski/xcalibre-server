import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        zinc: {
          50: "#fafafa",
          950: "#09090b",
        },
        teal: {
          600: "#0f766e",
        },
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        serif: ["Literata", "Georgia", "serif"],
      },
    },
  },
  plugins: [],
} satisfies Config;
