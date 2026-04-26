#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/backup.sh [--db-only] [--files-only]

Backs up xcalibre-server database and/or book files.

Options:
  --db-only     Back up database only.
  --files-only  Back up book files only.
  -h, --help    Show this help message.

Environment:
  XCS_BACKUP_DIR   Backup root directory (default: ./backups)
  XCS_DB_PATH      SQLite DB file path override
  XCS_STORAGE_PATH Storage directory override
  DATABASE_URL           Database URL (mysql://... enables MariaDB mode)
  APP_DATABASE_URL       Alternate database URL env name
  CONFIG_PATH            Config file path (default: config.toml)
USAGE
}

on_error() {
  local exit_code=$?
  echo "Error: backup failed." >&2
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

DB_ONLY=false
FILES_ONLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db-only)
      DB_ONLY=true
      shift
      ;;
    --files-only)
      FILES_ONLY=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$DB_ONLY" == true && "$FILES_ONLY" == true ]]; then
  echo "Error: --db-only and --files-only are mutually exclusive." >&2
  exit 1
fi

RUN_DB=true
RUN_FILES=true
if [[ "$DB_ONLY" == true ]]; then
  RUN_FILES=false
fi
if [[ "$FILES_ONLY" == true ]]; then
  RUN_DB=false
fi

CONFIG_FILE="${CONFIG_PATH:-config.toml}"
BACKUP_DIR="${XCS_BACKUP_DIR:-./backups}"
DB_PATH="${XCS_DB_PATH:-}"
STORAGE_PATH="${XCS_STORAGE_PATH:-}"
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

mkdir -p "$BACKUP_DIR"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"

DB_BACKUP_PATH=""
FILES_BACKUP_PATH=""

if [[ "$RUN_DB" == true ]]; then
  if [[ "$DB_MODE" == "sqlite" ]]; then
    require_cmd sqlite3
    DB_BACKUP_PATH="${BACKUP_DIR}/db-${TIMESTAMP}.db"
    sqlite3 "$DB_PATH" ".backup ${DB_BACKUP_PATH}"
  else
    require_cmd mysqldump
    require_cmd gzip
    parse_mysql_url "$DATABASE_URL"

    DB_BACKUP_PATH="${BACKUP_DIR}/db-${TIMESTAMP}.sql.gz"

    if [[ -n "$MARIADB_PASS" ]]; then
      MYSQL_PWD="$MARIADB_PASS" mysqldump \
        --single-transaction \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB" | gzip > "$DB_BACKUP_PATH"
    else
      mysqldump \
        --single-transaction \
        --host="$MARIADB_HOST" \
        --port="$MARIADB_PORT" \
        --user="$MARIADB_USER" \
        "$MARIADB_DB" | gzip > "$DB_BACKUP_PATH"
    fi
  fi
fi

if [[ "$RUN_FILES" == true ]]; then
  require_cmd rsync
  if [[ ! -d "$STORAGE_PATH" ]]; then
    echo "Error: storage path does not exist or is not a directory: $STORAGE_PATH" >&2
    exit 1
  fi

  FILES_BACKUP_PATH="${BACKUP_DIR}/files"
  mkdir -p "$FILES_BACKUP_PATH"
  rsync -a --delete "${STORAGE_PATH%/}/" "${FILES_BACKUP_PATH%/}/"
fi

echo "Backup complete."
if [[ -n "$DB_BACKUP_PATH" ]]; then
  echo "- Database (${DB_MODE}): $DB_BACKUP_PATH"
fi
if [[ -n "$FILES_BACKUP_PATH" ]]; then
  echo "- Book files: $FILES_BACKUP_PATH"
fi
echo "- Backup root: $BACKUP_DIR"
