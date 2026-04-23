#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/restore.sh <backup-db-file> [--files-dir <path>]

Restores autolibre database and optionally book files.

Arguments:
  <backup-db-file>  Backup file path (.db, .sql, or .sql.gz)

Options:
  --files-dir <path>  Restore book files from this directory.
  -h, --help          Show this help message.

Environment:
  AUTOLIBRE_DB_PATH       SQLite DB file path override
  AUTOLIBRE_STORAGE_PATH  Storage directory override
  DATABASE_URL            Database URL (mysql://... enables MariaDB mode)
  APP_DATABASE_URL        Alternate database URL env name
  CONFIG_PATH             Config file path (default: config.toml)
USAGE
}

on_error() {
  local exit_code=$?
  echo "Error: restore failed." >&2
  exit "$exit_code"
}
trap on_error ERR

parse_toml_value() {
  local file="$1"
  local section="$2"
  local key="$3"

  awk -v section="$section" -v key="$key" '
    BEGIN { current = "" }
    {
      line = $0
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      if (line == "" || line ~ /^#/) {
        next
      }

      if (line ~ /^\[/) {
        gsub(/^[[:space:]]*\[/, "", line)
        sub(/\][[:space:]]*$/, "", line)
        current = line
        next
      }

      if (current != section) {
        next
      }

      eq = index(line, "=")
      if (eq == 0) {
        next
      }

      k = substr(line, 1, eq - 1)
      gsub(/[[:space:]]+/, "", k)
      if (k != key) {
        next
      }

      value = substr(line, eq + 1)
      sub(/[[:space:]]*#.*/, "", value)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      if (value ~ /^".*"$/) {
        sub(/^"/, "", value)
        sub(/"$/, "", value)
      }
      print value
      exit
    }
  ' "$file"
}

sqlite_path_from_url() {
  local url="$1"
  local path

  path="${url#sqlite://}"
  path="${path%%\?*}"

  if [[ -z "$path" ]]; then
    return 1
  fi

  printf '%s\n' "$path"
}

parse_mysql_url() {
  local url="$1"
  local no_scheme auth_and_host db_and_query auth host_port

  no_scheme="${url#mysql://}"
  auth_and_host="${no_scheme%%/*}"
  db_and_query="${no_scheme#*/}"

  MARIADB_DB="${db_and_query%%\?*}"

  if [[ "$auth_and_host" == *"@"* ]]; then
    auth="${auth_and_host%@*}"
    host_port="${auth_and_host#*@}"
  else
    auth=""
    host_port="$auth_and_host"
  fi

  if [[ -n "$auth" ]]; then
    if [[ "$auth" == *":"* ]]; then
      MARIADB_USER="${auth%%:*}"
      MARIADB_PASS="${auth#*:}"
    else
      MARIADB_USER="$auth"
      MARIADB_PASS=""
    fi
  else
    MARIADB_USER=""
    MARIADB_PASS=""
  fi

  if [[ "$host_port" == *":"* ]]; then
    MARIADB_HOST="${host_port%:*}"
    MARIADB_PORT="${host_port##*:}"
  else
    MARIADB_HOST="$host_port"
    MARIADB_PORT="3306"
  fi

  if [[ -z "$MARIADB_HOST" || -z "$MARIADB_DB" ]]; then
    echo "Error: could not parse DATABASE_URL: $url" >&2
    exit 1
  fi
}

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Error: required command not found: $cmd" >&2
    exit 1
  fi
}

if [[ $# -eq 0 ]]; then
  usage >&2
  exit 1
fi

BACKUP_DB_FILE=""
FILES_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --files-dir)
      if [[ $# -lt 2 ]]; then
        echo "Error: --files-dir requires a path." >&2
        exit 1
      fi
      FILES_DIR="$2"
      shift 2
      ;;
    --*)
      echo "Error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [[ -n "$BACKUP_DB_FILE" ]]; then
        echo "Error: multiple backup DB files provided." >&2
        usage >&2
        exit 1
      fi
      BACKUP_DB_FILE="$1"
      shift
      ;;
  esac
done

if [[ -z "$BACKUP_DB_FILE" ]]; then
  echo "Error: missing <backup-db-file>." >&2
  usage >&2
  exit 1
fi

if [[ ! -f "$BACKUP_DB_FILE" ]]; then
  echo "Error: backup DB file not found: $BACKUP_DB_FILE" >&2
  exit 1
fi

CONFIG_FILE="${CONFIG_PATH:-config.toml}"
DB_PATH="${AUTOLIBRE_DB_PATH:-}"
STORAGE_PATH="${AUTOLIBRE_STORAGE_PATH:-}"
DATABASE_URL="${DATABASE_URL:-${APP_DATABASE_URL:-}}"

if [[ -r "$CONFIG_FILE" ]]; then
  if [[ -z "$DATABASE_URL" ]]; then
    DATABASE_URL="$(parse_toml_value "$CONFIG_FILE" "database" "url" || true)"
  fi

  if [[ -z "$STORAGE_PATH" ]]; then
    STORAGE_PATH="$(parse_toml_value "$CONFIG_FILE" "app" "storage_path" || true)"
  fi
fi

DB_MODE="sqlite"
if [[ "$DATABASE_URL" == mysql://* ]]; then
  DB_MODE="mariadb"
fi

if [[ "$DB_MODE" == "sqlite" ]]; then
  if [[ -z "$DB_PATH" && -n "$DATABASE_URL" && "$DATABASE_URL" == sqlite://* ]]; then
    DB_PATH="$(sqlite_path_from_url "$DATABASE_URL")"
  fi

  if [[ -z "$DB_PATH" ]]; then
    DB_PATH="library.db"
  fi
fi

if [[ -z "$STORAGE_PATH" ]]; then
  STORAGE_PATH="./storage"
fi

if [[ "$DB_MODE" == "sqlite" ]]; then
  if [[ -e "$DB_PATH" ]]; then
    read -r -p "Target DB file '$DB_PATH' exists. Overwrite? [y/N]: " answer
    case "$answer" in
      y|Y|yes|YES)
        ;;
      *)
        echo "Restore aborted."
        exit 1
        ;;
    esac
  fi

  mkdir -p "$(dirname "$DB_PATH")"
  cp "$BACKUP_DB_FILE" "$DB_PATH"
else
  require_cmd mysql
  parse_mysql_url "$DATABASE_URL"

  if [[ "$BACKUP_DB_FILE" == *.gz ]]; then
    require_cmd gunzip
    if [[ -n "$MARIADB_PASS" ]]; then
      MYSQL_PWD="$MARIADB_PASS" gunzip -c "$BACKUP_DB_FILE" | mysql \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB"
    else
      gunzip -c "$BACKUP_DB_FILE" | mysql \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB"
    fi
  else
    if [[ -n "$MARIADB_PASS" ]]; then
      MYSQL_PWD="$MARIADB_PASS" mysql \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB" < "$BACKUP_DB_FILE"
    else
      mysql \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB" < "$BACKUP_DB_FILE"
    fi
  fi
fi

if [[ -n "$FILES_DIR" ]]; then
  require_cmd rsync

  if [[ ! -d "$FILES_DIR" ]]; then
    echo "Error: files directory does not exist: $FILES_DIR" >&2
    exit 1
  fi

  mkdir -p "$STORAGE_PATH"
  rsync -a "${FILES_DIR%/}/" "${STORAGE_PATH%/}/"
fi

echo "Restore complete. Start the server with: docker compose up -d"
