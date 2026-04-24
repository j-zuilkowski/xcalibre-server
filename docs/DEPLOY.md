# Deploying autolibre

## Prerequisites
- Docker 24+ and Docker Compose v2
- A domain name pointing at your server (for TLS)
- 1GB RAM minimum; 4GB recommended for Meilisearch

## Key Rotation

This release changes the HKDF salts used to derive the AES-256-GCM keys for
TOTP secrets and webhook secrets. Fresh installs can deploy normally.

If you already have encrypted TOTP or webhook data in production, run a one-time
rotation before or during the rollout:

1. Back up the database and stop the app.
2. Read every non-null `users.totp_secret` value and every `webhooks.secret`
   value.
3. Decrypt each value with the legacy key derivation path that used `salt=None`.
4. Re-encrypt TOTP secrets with the new TOTP salt and webhook secrets with the
   new webhook salt.
5. Write the updated ciphertext back to the same rows.
6. Start the new release after the data has been rewritten.

If you do not have any existing TOTP or webhook secrets, you can skip this
rotation.

## Tier 1: Single Instance (SQLite) — Recommended for < 5 users

This is the standard self-hosted deployment. One container, SQLite DB, local filesystem or S3 for book files. Easiest to operate.

### Quick start

1. Clone the repository and enter it:

```bash
git clone https://github.com/<your-org>/autolibre.git
cd autolibre
```

2. Copy the example config:

```bash
cp config.example.toml config.toml
```

3. Generate a JWT secret (base64, 32 bytes):

```bash
openssl rand -base64 32
```

4. Edit `config.toml` and set at minimum:
- `auth.jwt_secret = "<output from openssl rand -base64 32>"`
- `admin_password` (use this as your first admin password when creating the initial admin account)
- `app.library_name = "My Library"`
- `app.storage_path = "./storage"`

5. Start the stack from the repo root:

```bash
docker compose -f docker/docker-compose.yml up -d
```

Expected output is similar to:

```text
[+] Running 3/3
 ✔ Network docker_default         Created
 ✔ Volume docker_library_data     Created
 ✔ Container docker-app-1         Started
```

6. Create the first admin account:

```bash
curl -sS -X POST http://127.0.0.1:8083/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","email":"admin@example.com","password":"<admin_password>"}'
```

### Caddy reverse proxy (TLS)

Use this `Caddyfile` for one domain, automatic HTTPS, gzip/zstd compression, and a static `/covers/` bypass:

```caddy
library.example.com {
    encode zstd gzip

    @covers path /covers/*
    handle @covers {
        root * /srv/autolibre/storage
        file_server
        header Cache-Control "public, max-age=86400"
    }

    handle {
        reverse_proxy app:8083
    }
}
```

If you use the `caddy` service from Compose, mount storage read-only into Caddy so `/covers/` can be served directly:

```yaml
caddy:
  image: caddy:2-alpine
  ports: ["80:80", "443:443"]
  volumes:
    - ./Caddyfile:/etc/caddy/Caddyfile:ro
    - library_data:/srv/autolibre/storage:ro
    - caddy_data:/data
```

The `/metrics` endpoint is unauthenticated and should never be exposed publicly. Block it at the reverse proxy or restrict it to internal networks only:

```caddy
@metrics path /metrics
respond @metrics 403
```

Or, if the proxy is exposed to a wider network, allow only RFC1918 sources:

```caddy
@metrics {
    path /metrics
    not remote_ip 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16
}
respond @metrics 403
```

Nginx equivalent:

```nginx
server {
    listen 80;
    server_name library.example.com;

    gzip on;
    gzip_types text/plain text/css application/json application/javascript application/xml image/svg+xml;

    location /covers/ {
        alias /srv/autolibre/storage/covers/;
        try_files $uri =404;
        expires 1d;
        add_header Cache-Control "public, max-age=86400";
    }

    location / {
        proxy_pass http://app:8083;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Enabling Meilisearch (optional)

autolibre works without Meilisearch and falls back to SQLite FTS5.

1. Uncomment (or keep enabled) the `meilisearch` service in `docker/docker-compose.yml`.
2. Set Meilisearch in `config.toml`:

```toml
[meilisearch]
enabled = true
url = "http://meilisearch:7700"
api_key = "${MEILI_MASTER_KEY}"
```

3. Set environment variables before startup:

```bash
export MEILI_MASTER_KEY="$(openssl rand -hex 32)"
export APP_MEILISEARCH_ENABLED=true
export APP_MEILISEARCH_API_KEY="$MEILI_MASTER_KEY"
```

4. Restart:

```bash
docker compose -f docker/docker-compose.yml up -d
```

### Enabling S3 storage (optional)

Use this exact `config.toml` structure:

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "my-autolibre-library"
region = "us-east-1"
endpoint_url = ""
access_key = "YOUR_ACCESS_KEY"
secret_key = "YOUR_SECRET_KEY"
key_prefix = "autolibre/"
```

`endpoint_url` examples:
- MinIO: `http://minio.local:9000`
- Cloudflare R2: `https://<account_id>.r2.cloudflarestorage.com`
- Backblaze B2 S3 API: `https://s3.<region>.backblazeb2.com`

One-time migration from local filesystem to S3:
1. Stop the server.
2. `aws s3 sync {storage_path}/ s3://{bucket}/ --delete`
3. Update `config.toml`: `backend = "s3"`
4. Restart the server.
5. Verify a book download works.

## Tier 2: Multi-Instance (MariaDB) — For multi-user or HA setups

When to use this: more than ~20 concurrent users, or when you need zero-downtime deploys (multiple app replicas with a shared DB).

### MariaDB setup

Compose snippet:

```yaml
services:
  mariadb:
    image: mariadb:11
    restart: unless-stopped
    environment:
      MARIADB_DATABASE: autolibre
      MARIADB_USER: autolibre
      MARIADB_PASSWORD: change-this
      MARIADB_ROOT_PASSWORD: change-root-password
    command: ["--max-connections=200"]
    volumes:
      - mariadb_data:/var/lib/mysql

  app:
    environment:
      APP_DATABASE_URL: mysql://autolibre:change-this@mariadb:3306/autolibre
    depends_on:
      - mariadb

volumes:
  mariadb_data:
```

Database initialization SQL:

```sql
CREATE DATABASE autolibre;
GRANT ALL ON autolibre.* TO 'autolibre'@'%';
```

Config change:

```toml
[database]
url = "mysql://autolibre:password@mariadb:3306/autolibre"
```

Recommended MariaDB server setting: `max_connections=200` (increase if you run multiple replicas and heavy background jobs).

### Multiple app replicas

Swarm/Compose deploy snippet:

```yaml
services:
  app:
    deploy:
      replicas: 2
```

For plain Docker Compose (non-Swarm), use:

```bash
docker compose -f docker/docker-compose.production.yml up -d --scale app=2
```

All replicas must share:
- The same `config.toml` (JWT secret must match across replicas)
- The same storage backend (use S3 — local filesystem does not work with multiple replicas)

Warning: SQLite cannot be used with multiple replicas.

### Health check endpoint

Use `GET /health` for load balancer health checks. Expected response: `200 {"status":"ok"}`.

Caddy upstream health check example:

```caddy
library.example.com {
    reverse_proxy app-1:8083 app-2:8083 {
        health_uri /health
        health_interval 10s
        health_timeout 2s
    }
}
```

## Backup and Restore

### SQLite backup

Recommended DB backup command (online backup):

```bash
sqlite3 library.db ".backup backup-$(date +%Y%m%d-%H%M%S).db"
```

Automate with cron (daily at 03:15):

```cron
15 3 * * * cd /opt/autolibre && sqlite3 library.db ".backup backup-$(date +\%Y\%m\%d-\%H\%M\%S).db"
```

Also back up book files (`storage_path`):

```bash
rsync -a --delete /opt/autolibre/storage/ /opt/autolibre/backups/files/
```

### MariaDB backup

```bash
mysqldump --single-transaction -h mariadb -u autolibre -p autolibre | gzip > autolibre-$(date +%Y%m%d).sql.gz
```

### Restore procedure

SQLite restore:
1. Stop server: `docker compose -f docker/docker-compose.yml down`
2. Restore DB: `cp /path/to/backup.db /path/to/library.db`
3. Restore files: `rsync -a /path/to/files/ /path/to/storage/`
4. Start server: `docker compose -f docker/docker-compose.yml up -d`
5. Verify login, search, and one book download.

MariaDB restore:
1. Stop server: `docker compose -f docker/docker-compose.production.yml down`
2. Restore DB: `gunzip -c /path/to/autolibre-YYYYMMDD.sql.gz | mysql -h mariadb -u autolibre -p autolibre`
3. Restore files: `rsync -a /path/to/files/ /path/to/storage/`
4. Start server: `docker compose -f docker/docker-compose.production.yml up -d`
5. Verify login, search, and one book download.

### S3 backup (book files)

If using S3, book files are durable by design. Enable bucket versioning:

```bash
aws s3api put-bucket-versioning \
  --bucket my-autolibre-library \
  --versioning-configuration Status=Enabled
```

## Upgrade Procedure

1. Pull new image:

```bash
docker compose -f docker/docker-compose.production.yml pull
```

2. Check `CHANGELOG.md` for migration notes.
3. Migrations run automatically on startup — no manual step required.
4. Restart:

```bash
docker compose -f docker/docker-compose.production.yml up -d
```

5. Verify:

```bash
docker compose -f docker/docker-compose.production.yml logs -f | head -50
```

Downgrade is not supported. Always back up before upgrading.

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---|---|---|
| "Database locked" errors | SQLite WAL file not cleaned up | Restart the container; check for stuck processes |
| Covers not loading | `storage_path` not mounted in Docker | Add volume mount in `docker-compose.yml` |
| OPDS feeds returning 401 | OPDS auth not configured | Check `opds.require_auth` in `config.toml` |
| Meilisearch not indexing | `MEILI_MASTER_KEY` mismatch | Must match between app config and Meilisearch container |
| LDAP auth failing | LDAP server unreachable | App falls back to local auth; check `ldap.host` and network |
| S3 uploads failing | Credentials or bucket policy | Check `access_key`, `secret_key`, and bucket IAM policy |
