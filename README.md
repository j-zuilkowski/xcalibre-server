# xcalibre-server

Self-hosted ebook library manager. Rust backend, React web app, native iOS/Android apps.

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/xcalibre/xcalibre-server)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Latest Release](https://img.shields.io/badge/release-v2.2.0-blue)](https://github.com/xcalibre/xcalibre-server/releases)
[![Docker Pulls](https://img.shields.io/badge/docker%20pulls-10k+-success)](https://hub.docker.com/r/xcalibre/xcalibre-server)

---

## What is xcalibre-server?

xcalibre-server is a modern rewrite of calibre-web, built in Rust for speed, reliability, and minimal resource footprint. It lets you host your own ebook library on your own hardware — no cloud, no subscriptions, no privacy compromises.

Access your books through a responsive web browser, native iOS/Android apps, or e-reader devices. Browse by author, series, or custom tags. Read EPUB, PDF, comics, and more directly in your browser or app. Sync reading progress across devices. Invite family members to share your library with fine-grained permission controls. All your data stays on your network.

xcalibre-server is purpose-built to run on constrained hardware: a Raspberry Pi 4, a Synology NAS, or a home server. A single-binary Rust backend and static React frontend mean a ~25MB Docker image, ~100MB RAM at idle (half the size of Python alternatives), and zero external runtime dependencies.

Designed as a direct successor to calibre-web with a seamless migration path. Unlike the original, xcalibre-server includes native mobile apps, bidirectional e-reader sync, AI-powered organization (optional, local-only), and comprehensive full-text search out of the box.

---

## Features

| | |
|---|---|
| **Browse library** | Grid or list view with covers, filters by author/series/tags, full-text search |
| **Read in browser** | EPUB, PDF, CBZ/CBR comics, DJVU, and audiobook streaming with persistent reading progress |
| **Multi-device sync** | Pause on phone, resume on tablet, pick up on e-reader — all in sync |
| **Native apps** | iOS and Android with offline reading, annotations, and shelf management |
| **Multi-user** | Invite family/friends; each gets their own reading history, shelves, and permissions |
| **E-reader support** | Kobo direct sync (bidirectional progress + shelves), OPDS catalog (Moon+ Reader, FBReader, Kybook, etc.), Send to Kindle via SMTP |
| **Annotations** | Highlight text, add notes, bookmark pages (web and mobile) |
| **Library management** | Bulk upload, metadata editing, cover management with auto-extraction, auto-tagging by genre/subject |
| **Import from Calibre** | One-command migration of existing Calibre libraries (dry-run available) |
| **Shelves** | Create personal reading lists and curated collections, share with other users |
| **Search** | Full-text search via Meilisearch (optional) or SQLite FTS5 fallback; semantic search (with AI features) |
| **In-progress reading** | Dedicated "Continue Reading" row showing books with active reading progress |
| **Metadata enrichment** | Search Google Books and Open Library to pull covers, descriptions, and identifiers for any book |
| **Multi-library** | Manage multiple independent book collections with per-user default library |
| **Localization** | English, French, German, Spanish |
| **AI features (optional)** | Auto-tagging, semantic search, metadata validation, cross-document synthesis (runsheets, specs, recipes, study guides) — all local, disabled by default |
| **MCP server** | Agentic AI integration for programmatic library access |
| **S3 storage** | S3-compatible backend for book files (optional alternative to local filesystem) |

---

## Quick Start

The fastest way to run xcalibre-server is with Docker Compose.

**Prerequisites:** Docker (v24+) and Docker Compose v2

**Steps:**

```bash
# Download the compose file
curl -O https://raw.githubusercontent.com/xcalibre/xcalibre-server/main/docker/docker-compose.yml

# Start the services
docker compose up -d

# Open http://localhost:8083 and create your admin account
```

Full installation guide with Synology NAS setup, HTTPS with Caddy, bare-metal install, and more: [docs/USER_GUIDE.md](docs/USER_GUIDE.md)

---

## Importing from Calibre

xcalibre-server includes a migration tool (`xs-migrate`) for importing existing Calibre libraries in a single command. The tool supports dry-run mode to preview the migration before committing. See [Importing Your Library](docs/USER_GUIDE.md#importing-your-library) in the User Guide.

---

## Screenshots

<!-- Screenshots -->

*(Screenshots coming soon — home page, library grid, EPUB reader, book detail with metadata identify, mobile app)*

---

## AI Features (Optional)

xcalibre-server includes optional AI capabilities for enhanced library management — all disabled by default. These features unlock:

- **Auto-tagging** — Automatically classify and tag books by genre, subject, and reading level
- **Semantic search** — Find books by concept and meaning, not just keywords
- **Metadata validation** — Verify and fix incomplete book metadata
- **Cross-document synthesis** — Generate runsheets, recipes, design specs, study guides, and other structured output from library content

All AI inference runs locally on your own hardware. Book content never leaves your network. Requires an OpenAI-compatible LLM server (LM Studio, Ollama, or llama.cpp). See [docs/LLM_GUIDE.md](docs/LLM_GUIDE.md) for model recommendations, setup instructions, and performance tuning.

---

## Documentation

| Document | Description |
|---|---|
| [User Guide](docs/USER_GUIDE.md) | Installation, setup, daily use, and all features |
| [LLM Guide](docs/LLM_GUIDE.md) | AI features, model recommendations, local server setup |
| [Architecture](docs/ARCHITECTURE.md) | System design, decisions, v1.0 scope |
| [API Reference](docs/API.md) | REST API contract |
| [Developer Guide](docs/DEVELOPER_GUIDE.md) | Contributing, codebase structure, extensibility |
| [Security](docs/SECURITY.md) | Security model and hardening |
| [Deployment](docs/DEPLOY.md) | Docker, Synology, bare metal, HTTPS |
| [MCP Server](docs/MCP.md) | Agentic AI integration |
| [Changelog](docs/CHANGELOG.md) | Version history |

---

## Hardware Requirements

| Hardware | Minimum | Recommended |
|---|---|---|
| **Raspberry Pi** | Pi 4, 1GB RAM | Pi 4, 4GB RAM |
| **NAS** | Synology/TrueNAS/QNAP with 1GB RAM | 2GB+ RAM, optional GPU |
| **Desktop/Server** | 512MB RAM | 2GB+ RAM for smooth search |

**Note:** Meilisearch (optional full-text search) uses ~200MB RAM. If not enabled, xcalibre-server falls back to SQLite FTS5 (lower performance, no additional overhead).

---

## Stack

**Backend:** Rust, Axum web framework, sqlx with compile-time query checking, SQLite (default) or MariaDB (optional)

**Frontend:** React, Vite, TanStack Router, shadcn/ui component library, react-i18next

**Mobile:** Expo (React Native), iOS + Android, native offline support

**Search:** Meilisearch (optional) + SQLite FTS5 (fallback)

**Authentication:** JWT, argon2id password hashing, TOTP two-factor authentication, OAuth (Google + GitHub), LDAP

**Deployment:** Docker images for amd64, arm64, armv7 (Raspberry Pi) — single binary, ~25MB compressed

---

## License

MIT

---

## Acknowledgements

xcalibre-server is a ground-up rewrite. It is not affiliated with the Calibre or calibre-web projects.
