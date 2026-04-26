# xcalibre-server User Guide

Welcome to xcalibre-server — a self-hosted ebook library manager. This guide will help you install, set up, and use xcalibre-server to manage your personal book collection.

---

## Table of Contents

1. [Introduction](#introduction)
2. [Installation](#installation)
3. [First-Run Setup](#first-run-setup)
4. [Importing Your Library](#importing-your-library)
5. [Using the Library](#using-the-library)
6. [Your Profile](#your-profile)
7. [User Management (Admin)](#user-management-admin)
8. [Kobo E-Reader Setup](#kobo-e-reader-setup)
9. [Send to Kindle (Email Delivery)](#send-to-kindle-email-delivery)
10. [OPDS Catalog](#opds-catalog)
11. [Multiple Libraries](#multiple-libraries)
12. [Mobile App](#mobile-app)
13. [AI Features (Optional)](#ai-features-optional)
14. [Backup and Maintenance](#backup-and-maintenance)
15. [Troubleshooting](#troubleshooting)

---

## Introduction

### What is xcalibre-server?

xcalibre-server is a modern rewrite of calibre-web, built in Rust for speed and reliability. It lets you host your own ebook library on your own hardware — no cloud, no subscriptions. You can access your books through a web browser or mobile app, read them online, track your reading progress, sync with e-readers like Kobo, and share books with other users on your network.

### Key Features

- **Browse your library** — Grid or list view with covers, filters by author/series/tags, full-text search
- **Read in your browser** — EPUB, PDF, CBZ/CBR comics, DJVU, and audiobook streaming
- **Reading progress sync** — Pause on one device, resume on another (web, mobile, or Kobo)
- **Multi-user** — Invite family or roommates; each has their own reading history and shelves
- **Kobo e-reader sync** — Your library syncs directly to your Kobo device
- **Send to Kindle** — Email books in any format your Kindle supports
- **OPDS catalog** — Compatible with any e-reader app (Moon+ Reader, Kybook, FBReader, etc.)
- **Shelves** — Create personal reading lists and share them with others
- **Annotations** — Highlight text, add notes, bookmark pages (web and mobile)
- **Optional AI features** — Auto-tagging, semantic search, metadata validation (requires local LLM)

### What You'll Need

**Hardware:**

| Tier | Specs | Best for |
|---|---|---|
| **Raspberry Pi 4** | ARMv7/ARM64, 1GB+ RAM | Small personal library (< 1,000 books), no AI features |
| **Home NAS** (Synology, TrueNAS, QNAP) | 2GB+ RAM, Docker support | Medium library (1,000–10,000 books), shared access |
| **Desktop/Server** | 2GB+ RAM, any Linux/Mac | Any size library, optional Meilisearch for faster search |

**Software:**

- Docker and Docker Compose v2 (recommended for all platforms)
- Or: download a pre-built binary and run it directly

### How It Compares to calibre-web

| Feature | calibre-web | xcalibre-server |
|---|---|---|
| **Language** | Python/Flask | Rust (faster, smaller footprint) |
| **Performance** | Moderate | Fast — single binary, minimal RAM |
| **E-reader support** | OPDS only | OPDS + Kobo sync + Send-to-Kindle |
| **Mobile app** | Web-only | Native iOS + Android apps |
| **AI features** | None | Optional (auto-tagging, semantic search) |
| **Format conversion** | Built-in | Separate project (xcalibre) |

---

## Installation

### Docker Compose (Recommended)

Docker is the easiest way to get started — it handles all dependencies and runs on any OS.

**Prerequisites:**
- Docker installed (version 24+)
- Docker Compose v2

**Steps:**

1. Download or clone xcalibre-server:
   ```bash
   git clone https://github.com/yourusername/xcalibre-server.git
   cd xcalibre-server
   ```

2. Copy the example config:
   ```bash
   cp config.example.toml config.toml
   ```

3. Generate a random JWT secret:
   ```bash
   openssl rand -base64 32
   ```
   Copy the output; you'll use it in the next step.

4. Edit `config.toml` and set these values:
   ```toml
   [auth]
   jwt_secret = "PASTE_OUTPUT_FROM_STEP_3_HERE"

   [app]
   library_name = "My Library"
   storage_path = "./storage"
   
   [meilisearch]
   enabled = true
   url = "http://meilisearch:7700"
   ```

5. Start the stack:
   ```bash
   docker compose -f docker/docker-compose.yml up -d
   ```

   Expected output:
   ```
   [+] Running 3/3
    ✔ Network docker_default      Created
    ✔ Volume docker_library_data  Created
    ✔ Container docker-app-1      Started
   ```

6. Open your browser to `http://localhost:8083` and create your first admin account.

**What each service does:**

| Service | Purpose | Port | Can disable? |
|---|---|---|---|
| **app** | xcalibre-server server | 8083 | No — core service |
| **meilisearch** | Fast search index (optional) | 7700 | Yes — falls back to built-in SQLite search (slower) |
| **caddy** | HTTPS reverse proxy (optional) | 80, 443 | Yes — only needed if you want automatic SSL/TLS |

**Volumes:**

| Volume | Purpose |
|---|---|
| `library_data` | Your book files, database, and cover images. Mount this to a local path if you want to access files outside Docker. |
| `meili_data` | Meilisearch index (search index only — not critical; can be rebuilt). |

### Synology NAS

If you use a Synology NAS:

1. Install Docker via Synology Package Center.
2. Open Container Manager.
3. Create a project from `docker/docker-compose.yml`:
   - Create volumes for `library_data` and `meili_data` (or use local paths).
   - Set port 8083 for the app.
   - Set environment variables (see below).
4. Create the project.
5. Check logs to confirm startup.
6. Visit `http://your-nas-ip:8083` to create your admin account.

**Synology volume paths:**
- Store `library_data` in `/volume1/docker/xcalibre-server/library` or a shared folder.
- On Synology, shared folder paths start with `/volume1/...`.

(See DEPLOY.md for detailed Synology steps.)

### Bare Metal (Advanced)

If you want to run xcalibre-server without Docker:

1. Download the latest release binary from GitHub.
2. Create `config.toml` in the same directory (see Installation step 4 above).
3. Run the binary:
   ```bash
   ./xcalibre-server
   ```
4. Visit `http://localhost:8083` to create your admin account.

This is not recommended unless you're comfortable with manual updates and troubleshooting.

### Raspberry Pi 4 Notes

The xcalibre-server Docker image includes ARM support (`linux/arm64` and `linux/armv7`).

**RAM considerations:**
- Without Meilisearch: 512 MB minimum, 1 GB recommended
- With Meilisearch: 1.5 GB+ recommended

If RAM is tight, disable Meilisearch in `config.toml`:
```toml
[meilisearch]
enabled = false
```

This falls back to built-in SQLite full-text search, which is slower but uses much less memory.

---

## First-Run Setup

### Create Your Admin Account

The first time you visit `http://your-server:8083`, you'll be prompted to create an admin account. This is the only account created this way — all others are created via the admin panel.

Enter:
- **Username:** your login name
- **Email:** your email address
- **Password:** a strong password (can be changed later)

### Key Configuration Settings

Once logged in, some settings in `config.toml` are important:

#### `base_url`
The public URL where your server is accessible. Set this correctly for:
- **Kobo sync** — Kobo needs to know where to contact your server
- **OAuth login** — If using Google or GitHub login
- **Email links** — Send-to-Kindle includes a link back to your server

Example:
```toml
base_url = "https://library.mydomainname.com"  # Production
base_url = "http://192.168.1.100:8083"         # Local network
```

#### `storage.path`
Where your book files are stored. Inside Docker, this is `/app/storage` (already correct). If you mount a different volume, update this path.

#### `[email]` Section (Optional)
Set this up only if you want Send-to-Kindle:
```toml
[email]
smtp_host = "smtp.gmail.com"
smtp_port = 587
smtp_user = "your-email@gmail.com"
smtp_password = "your-app-password"  # Use App Passwords, not your real password
from_address = "your-email@gmail.com"
use_tls = true
```

After saving, test in Admin Panel → Email Settings.

#### `[llm]` Section (Optional)
Leave as-is for now:
```toml
[llm]
enabled = false
```

Enable this only if you have a local LLM running (see Section 13: AI Features).

### Admin Panel Overview

Click the **Admin** link (top right) to access:

- **Users** — Create/edit users, assign roles
- **System** — View app version, database size, storage usage
- **Email Settings** — Configure SMTP for Send-to-Kindle
- **API Tokens** — Create tokens for Kobo sync and OPDS
- **Jobs** — Monitor background LLM tasks
- **Scheduled Tasks** — Set up automatic backups or re-indexing
- **Libraries** — Manage multiple libraries (optional)

---

## Importing Your Library

### From Calibre (Migration)

If you have an existing Calibre library, xcalibre-server can import it.

**What gets imported:**
- Books, authors, series, tags
- Book covers
- Custom columns
- Reading progress (if stored in Calibre Web)

**What doesn't:**
- Passwords (users must reset)
- Format conversion settings

**Steps:**

1. Go to **Admin** → **Migration**.
2. Enter the path to your Calibre `metadata.db` file (e.g., `/mnt/calibre/library/metadata.db`).
3. Click **Dry Run** to preview what would be imported — no data is changed.
4. Review the preview; if it looks good, click **Import**.
5. The import runs in the background. Check Admin → Jobs for progress.
6. Once complete, your books will appear in the library.

For **multi-library setups**, use the library selector in the Migration UI to choose which library to import into.

### Uploading Books Manually

#### Single Upload

1. Click **+ Upload** (top right of the library page).
2. Drag and drop a file, or click to browse.
3. Supported formats: EPUB, PDF, MOBI, AZW3, CBZ, CBR, DJVU, MP3, M4B, OGG
4. xcalibre-server automatically extracts metadata (title, author, cover, description) from the file.
5. Review the extracted metadata; edit as needed.
6. Confirm to save.
7. If the ISBN or title+author match an existing book, you'll be warned (can choose to keep both or skip).

#### Bulk Import

**Admin only:**

1. Go to **Admin** → **Import**.
2. Choose **Upload ZIP** or **Server Path**:
   - **Upload ZIP:** Select a `.zip` file containing multiple book files
   - **Server Path:** Enter a folder path on your server (e.g., `/mnt/incoming/books/`)
3. Click **Dry Run** to preview.
4. Click **Import** to proceed.
5. Check Admin → Jobs for progress.

**Duplicate handling:** Books matching an existing ISBN or title+author are skipped or warned.

---

## Using the Library

### Browsing

**Grid View (default):**
- Click any book card to see full details.
- Hover over a card to see rating, language, series, and reading progress (thin teal bar at the bottom).

**Filters:**
- **Author, Series, Tags, Language, Format:** Click any filter chip at the top to narrow results.
- **Sort:** Click the sort dropdown (default: Title) to change to Author, Date Added, Rating, etc.
- **Search:** Use the search bar for quick results (shows top 5 matches; click "View all results" for full search page).

**List View:**
- Toggle between grid and list in the top right.

### Reading a Book

1. Click any book to open the **Book Detail** page.
2. Click the **Read** button to open in-browser reader.

**EPUB Reader:**
- Font: Choose Inter or Literata
- Font size: Adjust with + / - buttons
- Line height: Spacing between lines
- Colors: Light or night mode
- Your position is saved automatically

**PDF Reader:**
- Navigate pages with arrow keys or buttons
- Zoom with scroll or magnifying glass

**Comic Reader (CBZ/CBR):**
- Click to turn pages, or use arrow keys
- Swipe on mobile

**Audio Player (MP3, M4B, OGG):**
- Standard player controls (play, pause, skip, speed)
- Progress saves automatically

### Highlights and Annotations (EPUB only)

1. Select any text in the reader.
2. Choose **Highlight** or **Note**.
3. Pick a color (yellow, green, blue, pink).
4. (Optional) Add a text note.
5. View all annotations in the ☰ menu → **Annotations** tab.

Annotations sync across all your devices.

### Downloading Books

1. On the **Book Detail** page, click **Download**.
2. Choose a format (EPUB, PDF, MOBI, etc.).
3. The file downloads to your device.

### Searching

**Quick search:**
- Type in the search bar at the top of any page.
- Results show title, author, series matches.
- Click "View all results" for the full search page.

**Full search page:**
- **Library tab:** Full-text search across book titles, authors, descriptions.
- **Semantic tab:** (If AI features enabled) Find books by meaning — e.g., "books about survival in extreme cold" matches thematically related titles.

### Shelves (Reading Lists)

Create personal reading lists:

1. Sidebar → **Shelves** → **+ New Shelf**.
2. Name it (e.g., "To Read", "2024 Goals", "Sci-Fi Favorites").
3. Toggle **Public** if you want other users to see it.
4. Click **Create**.

**Add books to a shelf:**
- Open a book detail page → ••• (menu) → **Add to shelf** → choose shelf.

**View a shelf:**
- Click its name in the sidebar.
- Edit the shelf name or public status here.
- Remove books by clicking the X on their cards.

---

## Your Profile

Click the **user icon** (top right) to access your profile settings.

### Change Password

**Profile** → **Security** → **Change Password**

Enter your current password, then your new password twice. Click **Update**.

### Preferred Language

**Profile** → **Preferences** → **Language**

Choose English, French, German, or Spanish. The app will reload in your selected language.

### Theme (Light/Dark Mode)

Click the sun/moon icon in the top right to toggle between light and dark mode.

### Reading Statistics

**Profile** → **Statistics**

View:
- **Reading streak:** Consecutive days you've opened a book
- **Books this month:** How many you've finished
- **Top authors:** Most-read authors
- **Top tags:** Most-read genres/categories

### Active Sessions

**Profile** → **Security** → **Sessions**

See all devices where you're currently logged in. Click **Revoke** to log out a device remotely.

### Two-Factor Authentication (TOTP)

Adds a second layer of security. Optional but recommended.

**Enable TOTP:**

1. **Profile** → **Security** → **Two-Factor Authentication** → **Enable**.
2. Scan the QR code with an authenticator app (Google Authenticator, Authy, 1Password, etc.).
3. Enter the 6-digit code from your app.
4. **Save backup codes** somewhere safe (in case you lose your phone).

Once enabled, you'll be asked for a 6-digit code when logging in.

---

## User Management (Admin)

### Create a User

1. **Admin** → **Users** → **+ New User**.
2. Enter username, email, and password.
3. Choose role (Admin or User).
4. Toggle permissions:
   - **Can Upload:** Allow book uploads
   - **Can Edit:** Allow metadata editing
   - **Can Download:** Allow book downloads
5. Click **Create**.

The new user can now log in with their credentials.

### Reset a User's Password

1. **Admin** → **Users** → click the user.
2. Click **Force Password Reset**.
3. The user will be prompted to set a new password on next login.

### Disable/Delete a User

1. **Admin** → **Users** → click the user.
2. Toggle **Active** to disable (user can't log in, but data isn't deleted).
3. Click **Delete** to permanently remove the user and their reading history.

### Configure Role Permissions

Roles define what users can do:

1. **Admin** → **Roles** → click a role.
2. Toggle each permission:
   - **Can Upload:** Allow single/bulk book uploads
   - **Can Edit:** Allow metadata editing (title, author, tags, etc.)
   - **Can Download:** Allow file downloads
3. Click **Update**.

---

## Kobo E-Reader Setup

xcalibre-server can sync your library directly to a Kobo e-reader. Reading progress syncs both ways — start on Kobo, resume in the app (and vice versa).

**Prerequisites:**
- A Kobo e-reader (Elipsa, Sage, Clara, etc.)
- WiFi connection

**Steps:**

1. **Admin** → **API Tokens** → **+ Create Token**.
2. Name it (e.g., "My Kobo") and click **Create**.
3. Copy the token.
4. On your **Kobo device**:
   - Settings → **Account**
   - **Add a Library** (or "Connect to a library server")
5. Enter:
   ```
   http://your-server-ip:8083/kobo/{paste-token-here}
   ```
   Example: `http://192.168.1.100:8083/kobo/abc123def456`
6. Tap **Connect**.
7. Your library will sync to the device.

**How it works:**
- Books appear on your Kobo immediately.
- Reading progress syncs automatically when the device has WiFi.
- You can create collections on your Kobo; they sync back to xcalibre-server as shelves.

**Note:** Format conversion is not included. Books must already be in a format your Kobo supports (EPUB recommended).

---

## Send to Kindle (Email Delivery)

Email any book to your Kindle device.

**Prerequisites:**
- Admin must configure SMTP (see First-Run Setup).
- Your email address must be on your Amazon "Approved Personal Document Email List."

**Setup (Admin only):**

1. **Admin** → **Email Settings**.
2. Enter your SMTP details:
   - **SMTP Host:** e.g., `smtp.gmail.com`
   - **SMTP Port:** e.g., `587`
   - **Username & Password:** (for Gmail, use App Passwords, not your real password)
   - **From Address:** Your email
   - **Use TLS:** Toggle on
3. Click **Test** to verify.

**How to use:**

1. Open any book detail page.
2. Click ••• (menu) → **Send to Kindle**.
3. Enter your Kindle email address (e.g., `janedoe@kindle.com`).
4. Choose a format (PDF, MOBI, EPUB).
5. Click **Send**.

The book will arrive on your Kindle in a few seconds.

---

## OPDS Catalog

OPDS is a standard library catalog protocol. Any OPDS-compatible app can browse and download from your xcalibre-server library without logging in via the web.

**Compatible apps:**
- Moon+ Reader (Android)
- Kybook 3 (iOS)
- FBReader (iOS/Android)
- PocketBook devices
- Calibre (desktop)

**Your OPDS URL:**

```
http://your-server-ip:8083/opds
```

**To use with an app:**

1. Open your OPDS app.
2. Add a library → paste your OPDS URL.
3. Browse your library in the app.

**Downloads require an API token:**

1. **Admin** → **API Tokens** → **+ Create Token** (name it "OPDS").
2. In your OPDS app settings, append `?token={token}` to the download link pattern.
   - Example: `http://192.168.1.100:8083/opds/books/{id}/formats/{format}/download?token=abc123`

---

## Multiple Libraries

If you want to organize books into separate collections (e.g., "Personal", "Shared Family Library", "Work Reference"):

**Create a library:**

1. **Admin** → **Libraries** → **+ New Library**.
2. Enter a name (e.g., "Family Shared").
3. (Optional) Point to a Calibre database for import.
4. Click **Create**.

**Import books into a specific library:**

1. Go to **Admin** → **Migration**.
2. Select the library from the dropdown.
3. Proceed with migration.

**Switch between libraries:**

- Click the library name in the header (top left, next to "xcalibre-server").
- Choose which library to browse.
- Your default library is set in **Profile** → **Preferences** → **Default Library**.

---

## Mobile App

Download xcalibre-server from the **App Store** (iOS) or **Google Play Store** (Android). The app offers the same features as the web version on mobile.

**Features:**
- Browse your library (grid or list)
- Read books (EPUB, PDF, comics)
- Track reading progress (syncs with web version)
- Annotations (highlights, notes, bookmarks)
- Shelves (reading lists)
- Offline reading (download books to read without internet)

**Setup:**

1. Open the app.
2. Tap **Add Server**.
3. Enter your server URL (e.g., `http://192.168.1.100:8083` or `https://library.mydomainname.com`).
4. Log in with your username and password.

**Offline reading:**

1. Open a book.
2. Tap **Download** (icon at the top right).
3. The book is saved locally; you can read it without internet.
4. Downloaded books appear in the **Downloads** tab.

---

## AI Features (Optional)

xcalibre-server can optionally use a locally-running AI model for:

- **Auto-tagging:** AI suggests genre, subject, and reading level tags when you upload a book.
- **Semantic search:** Find books by meaning — "books about survival in extreme cold" returns thematically related titles.
- **Metadata validation:** AI checks for missing or incomplete metadata.
- **Content quality:** AI rates writing quality.
- **Book summaries:** AI generates summaries, discussion questions, related titles.

**Important:** AI features are **disabled by default** and require a locally-running LLM (via LM Studio or Ollama). See `docs/LLM_GUIDE.md` for setup details.

---

## Backup and Maintenance

### What to Back Up

Three things:

1. **The database:** `{storage_path}/xcalibre-server.db` (or wherever `database.path` points in `config.toml`)
2. **Book files:** Everything in `{storage_path}/`
3. **Configuration:** `config.toml`

### Back Up the Database

While the server is running:

```bash
docker exec xcalibre-server sqlite3 /app/storage/xcalibre-server.db ".backup /app/storage/xcalibre-server.backup.db"
cp /path/to/volume/xcalibre-server.backup.db /your/backup/location/
```

### Scheduled Backups (Admin)

1. **Admin** → **Scheduled Tasks** → **+ New Task**.
2. Set **Type** to `backup`.
3. Set **Cron expression** (e.g., `0 3 * * *` for 3 AM daily).
4. Click **Create**.

The database will back up automatically at the scheduled time.

### Restore from Backup

1. Stop the server: `docker compose down`
2. Restore the database: `cp /backup/xcalibre-server.db /path/to/volume/xcalibre-server.db`
3. Restore book files: `rsync -a /backup/files/ /path/to/volume/`
4. Start the server: `docker compose up -d`
5. Verify: log in, search, and download a book.

### Updates

When a new version is released:

1. **Admin** → **System** — shows current version and if an update is available.
2. Pull the latest image: `docker compose pull`
3. Restart: `docker compose up -d`

Migrations run automatically. No manual steps needed.

---

## Troubleshooting

| Problem | Cause | Solution |
|---|---|---|
| **Can't log in after first run** | Admin account not created | Visit `http://server:8083` and create it now |
| **Books not appearing after import** | Meilisearch not running or indexing | Check `docker compose ps`; search is optional; disable Meilisearch in `config.toml` to fall back to built-in search |
| **Kobo not syncing** | Wrong token or base_url | Verify token in Admin → API Tokens; confirm `base_url` in `config.toml` is correct |
| **Cover images not showing** | Storage path misconfigured | Check `storage.path` in `config.toml` matches your volume mount in docker-compose.yml |
| **"LLM unavailable" message** | LLM disabled or LM Studio not running | LLM features are optional; see `docs/LLM_GUIDE.md` to enable |
| **OPDS downloads failing** | No API token in URL | Create token in Admin → API Tokens; append `?token={token}` to download URLs |
| **Send to Kindle not working** | SMTP not configured or email not whitelisted | Configure SMTP in Admin → Email Settings; add your email to Amazon's Approved Senders list |
| **Slow search** | Meilisearch not fully indexed yet | Wait a few minutes after import; check Admin → System for Meilisearch status |
| **"Database locked" errors** | SQLite WAL file not cleaned up | Restart the container: `docker compose restart app` |
| **Can't upload books (permission denied)** | Upload permission not enabled for your role | Ask admin to enable "Can Upload" on your role in Admin → Roles |

---

## Getting Help

- **DEPLOY.md** — Detailed deployment and configuration guide
- **ARCHITECTURE.md** — Technical overview (for reference)
- **API.md** — API endpoints (for integrations)
- **docs/LLM_GUIDE.md** — Detailed guide to optional AI features

For issues or feature requests, see the GitHub repository.

---

_Last updated: April 2026. Version 1.0+_
