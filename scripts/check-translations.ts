#!/usr/bin/env tsx
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function loadLocale(code: string): Record<string, unknown> {
  const candidates = [
    path.resolve(__dirname, `../apps/web/src/locales/${code}.json`),
    path.resolve(__dirname, `../apps/web/public/locales/${code}/translation.json`),
  ];

  const file = candidates.find((candidate) => fs.existsSync(candidate));
  if (!file) {
    throw new Error(`Locale file for '${code}' not found. Checked: ${candidates.join(", ")}`);
  }

  return JSON.parse(fs.readFileSync(file, "utf8")) as Record<string, unknown>;
}

// Recursively collect all dot-notation key paths from a nested object
function collectKeys(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const keyPath = prefix ? `${prefix}.${k}` : k;
    return typeof v === "object" && v !== null && !Array.isArray(v)
      ? collectKeys(v as Record<string, unknown>, keyPath)
      : [keyPath];
  });
}

const en = loadLocale("en");
const fr = loadLocale("fr");
const de = loadLocale("de");
const es = loadLocale("es");

const enKeys = new Set(collectKeys(en));
const locales: [string, Record<string, unknown>][] = [
  ["fr", fr],
  ["de", de],
  ["es", es],
];

let exitCode = 0;

for (const [code, locale] of locales) {
  const localeKeys = new Set(collectKeys(locale));
  const missing = [...enKeys].filter((k) => !localeKeys.has(k));
  const extra = [...localeKeys].filter((k) => !enKeys.has(k));

  if (missing.length > 0) {
    console.error(`[${code}] Missing ${missing.length} key(s):`);
    missing.forEach((k) => console.error(`  - ${k}`));
    exitCode = 1;
  }

  if (missing.length === 0) {
    console.log(`[${code}] ✓ 100% coverage`);
  }

  if (extra.length > 0) {
    console.warn(`[${code}] ${extra.length} key(s) not in EN (orphaned):`);
    extra.forEach((k) => console.warn(`  + ${k}`));
  }
}

process.exit(exitCode);
