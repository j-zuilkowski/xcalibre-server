#!/usr/bin/env node
// MCP server for xcalibre-server development tooling
// Exposes cargo, db, and codex controls as Claude Code tools

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { execSync, exec } from "child_process";
import { promisify } from "util";
import { z } from "zod";
import path from "path";
import { fileURLToPath } from "url";

const execAsync = promisify(exec);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const BACKEND = path.join(ROOT, "backend");

const server = new McpServer({
  name: "xcalibre-server-dev",
  version: "1.0.0",
});

server.tool(
  "run_tests",
  "Run cargo tests. Optionally filter by test file (e.g. 'test_auth') or test name.",
  {
    filter: z.string().optional().describe("Test file or name filter e.g. 'test_auth'"),
    show_output: z.boolean().optional().default(true),
  },
  async ({ filter, show_output }) => {
    const filterArg = filter ? `--test ${filter}` : "--workspace";
    const cmd = `cd ${BACKEND} && cargo test ${filterArg} 2>&1`;
    try {
      const { stdout } = await execAsync(cmd, { timeout: 120000 });
      return { content: [{ type: "text", text: show_output ? stdout : "Tests passed." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

server.tool(
  "cargo_check",
  "Run cargo check to verify the project compiles without errors.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(`cd ${BACKEND} && cargo check 2>&1`, { timeout: 60000 });
      return { content: [{ type: "text", text: stdout || "cargo check passed." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

server.tool(
  "cargo_clippy",
  "Run cargo clippy with -D warnings. Returns warnings and errors.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(
        `cd ${BACKEND} && cargo clippy --workspace -- -D warnings 2>&1`,
        { timeout: 60000 }
      );
      return { content: [{ type: "text", text: stdout || "clippy clean." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

server.tool(
  "cargo_audit",
  "Run cargo audit to check for known CVEs in dependencies.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(`cd ${ROOT} && cargo audit 2>&1`, { timeout: 60000 });
      return { content: [{ type: "text", text: stdout || "No vulnerabilities found." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

server.tool(
  "db_query",
  "Run a read-only SQL query against the development SQLite database.",
  {
    sql: z.string().describe("SELECT query to run"),
    db_path: z.string().optional().default("./library.db").describe("Path to SQLite DB"),
  },
  async ({ sql, db_path }) => {
    const trimmed = sql.trim().toUpperCase();
    if (!trimmed.startsWith("SELECT") && !trimmed.startsWith("PRAGMA")) {
      return {
        content: [{ type: "text", text: "Only SELECT and PRAGMA statements allowed." }],
        isError: true,
      };
    }
    try {
      const { stdout } = await execAsync(
        `sqlite3 -column -header "${path.resolve(ROOT, db_path)}" "${sql.replace(/"/g, '\\"')}"`,
        { timeout: 10000 }
      );
      return { content: [{ type: "text", text: stdout || "(no rows)" }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.message }], isError: true };
    }
  }
);

server.tool(
  "list_tables",
  "List all tables in the development SQLite database with row counts.",
  {
    db_path: z.string().optional().default("./library.db"),
  },
  async ({ db_path }) => {
    try {
      const { stdout } = await execAsync(
        `sqlite3 "${path.resolve(ROOT, db_path)}" ".tables"`,
        { timeout: 10000 }
      );
      return { content: [{ type: "text", text: stdout || "(no tables — run migrations first)" }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.message }], isError: true };
    }
  }
);

server.tool(
  "run_migrations",
  "Run sqlx migrations against the development database.",
  {
    db_url: z.string().optional().default("sqlite://./library.db"),
  },
  async ({ db_url }) => {
    try {
      const { stdout } = await execAsync(
        `cd ${BACKEND} && DATABASE_URL="${db_url}" cargo sqlx migrate run 2>&1`,
        { timeout: 60000 }
      );
      return { content: [{ type: "text", text: stdout || "Migrations applied." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

const transport = new StdioServerTransport();
await server.connect(transport);
