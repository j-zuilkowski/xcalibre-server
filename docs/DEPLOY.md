# Deployment Guide

Three deployment paths are supported:

- Synology NAS (Docker)
- Generic Docker + Caddy
- Bare Metal

## Prerequisites

These apply to all paths:

- A domain name or static LAN IP
- One migration run to import your Calibre library with `xs-migrate`
- A generated `MEILI_MASTER_KEY`: `openssl rand -hex 32`

## Path 1: Synology NAS

Recommended for home users.

### Requirements

- DSM 7.2 or newer with Container Manager installed
- At least 2 GB free RAM
- Your Calibre library folder accessible on the NAS

### Steps

1. Install Container Manager from Synology Package Center.
2. Open Container Manager, choose Project, then Create.
3. Set the project path to a folder on the NAS, such as `/docker/calibre-web-rs`.
4. Place `docker/docker-compose.production.yml` and `config.toml` in the same folder.
5. Set environment variables:
   - `MEILI_MASTER_KEY`: your generated key
   - `GITHUB_REPOSITORY`: your fork, or leave unset to use the default image name
6. Start the project.
7. Open the app at `http://<NAS-IP>:8083`.
8. Run `xs-migrate` once to import your Calibre library.
9. For HTTPS, enable Synology reverse proxy at Control Panel -> Login Portal ->
   Advanced -> Reverse Proxy, then point your domain at `localhost:8083`.

### Upgrading

Use Container Manager -> Project -> your project -> Update.
No data is lost because all persistent data lives in named volumes.

## Path 2: Generic Docker + Caddy

Good for a VPS or a home server.

### Steps

1. Clone the repository or copy `docker/docker-compose.production.yml` and `docker/Caddyfile`.
2. Set `APP_DOMAIN` in your shell:

```bash
export APP_DOMAIN=library.yourdomain.com
```

3. Edit `docker/Caddyfile` and replace `{$APP_DOMAIN:localhost}` with your domain.
4. Put `config.toml` next to `docker/docker-compose.production.yml`.
5. Start the stack:

```bash
docker compose -f docker/docker-compose.production.yml up -d
```

6. Caddy will obtain a Let’s Encrypt certificate automatically on first start.
7. Run `xs-migrate` to import your Calibre library.

## Path 3: Bare Metal

Best for advanced users who want to manage the process directly.

### Requirements

- Rust 1.77 or newer
- SQLite 3.35 or newer
- Optional: Meilisearch binary if you want the hosted search service

### Steps

1. Build the backend:

```bash
cargo build --release -p backend
```

2. Copy `target/release/backend` to the server.
3. Create `config.toml`.
4. Run the binary as a systemd service.
5. Put Caddy or nginx in front for HTTPS.

### Systemd Service

```ini
[Unit]
Description=xcalibre-server
After=network.target

[Service]
Type=simple
User=calibre
WorkingDirectory=/opt/xcalibre-server
ExecStart=/opt/xcalibre-server/backend
Restart=on-failure
Environment=RUST_LOG=warn
Environment=APP_DATABASE__URL=sqlite:///opt/xcalibre-server/storage/library.db

[Install]
WantedBy=multi-user.target
```

## Configuration Reference

All settings live in `config.toml`. Environment variables with the `APP_`
prefix override file values. Use double underscores for nested keys, such as
`APP_DATABASE__URL`.

| Key | Default | Required | Description |
|---|---|---|---|
| `app.base_url` | - | Yes | Full URL the app is served at, for example `https://library.example.com` |
| `app.storage_path` | `./storage` | No | Location for book files and covers |
| `database.url` | `sqlite://./library.db` | No | SQLite path or MariaDB connection string |
| `auth.jwt_secret` | auto-generated | No | Min 256-bit random value; auto-generated if blank |
| `auth.access_token_ttl_mins` | `15` | No | JWT lifetime in minutes |
| `auth.refresh_token_ttl_days` | `30` | No | Refresh token lifetime in days |
| `auth.max_login_attempts` | `10` | No | Failed attempts before lockout |
| `llm.enabled` | `false` | No | Enables LLM features |
| `llm.embedding_model` | - | No | Separate model for embeddings (e.g. `nomic-embed-text-v1.5`); falls back to `llm.librarian.model` if unset |
| `llm.librarian.endpoint` | - | If `llm.enabled` | LM Studio or Ollama base URL |
| `network.allow_private_endpoints` | `false` | No | Allow LLM endpoints and webhook targets on LAN/private IPs (also settable as `llm.allow_private_endpoints` for backwards compatibility) |
| `llm.librarian.model` | auto | No | Model name; auto-discovered from `/v1/models` if blank |
| `limits.upload_max_bytes` | `524288000` | No | Max upload size, default 500 MB |

Optional Meilisearch settings follow the same `APP_MEILISEARCH_*` pattern.

## Migration

Run `xs-migrate` once after first install to import an existing Calibre library.
Start with a dry run, then run the real import:

```bash
# Dry run first
./xs-migrate --calibre-db /path/to/metadata.db --dry-run

# Import
./xs-migrate --calibre-db /path/to/metadata.db \
  --storage-path /app/storage \
  --db-url sqlite:///app/storage/library.db
```

The migration is idempotent and skips already imported records.

## Backup and Restore

### What to back up

- `library.db` - metadata, users, reading progress, and LLM job history
- `storage/` - book files and cover images
- `config.toml` - configuration, including the JWT secret

### Backup

```bash
sqlite3 library.db ".backup library.db.bak"
tar czf storage.tar.gz storage/
```

### Restore

```bash
cp library.db.bak library.db
tar xzf storage.tar.gz
```

### Upgrade Procedure

1. Back up `library.db` and `storage/`.
2. Pull the new image:

```bash
docker compose -f docker/docker-compose.production.yml pull
```

3. Restart the stack:

```bash
docker compose -f docker/docker-compose.production.yml up -d
```

4. Check the app logs:

```bash
docker compose -f docker/docker-compose.production.yml logs app --tail=50
```

5. If migration fails, restore from backup and report the issue.
