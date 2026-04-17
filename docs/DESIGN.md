# calibre-web Rewrite — UI/UX Design Specification

_Status: Draft_
_Last updated: 2026-04-17_

---

## Design Philosophy

**Progressive disclosure.** Every screen has one job. Advanced options exist but are never in the way. Top-level surfaces are clean — complexity lives one level deeper, revealed on demand.

**Card-first.** Cover art is the primary identifier for a book. The library is a visual experience, not a table of data.

**Equal weight.** The reading experience is as polished as the library browse. Neither is an afterthought.

---

## Principles

1. **One primary action per screen** — everything else is secondary or hidden until needed
2. **Nothing on the surface that most users won't use most of the time**
3. **Admin and LLM features are never visible to non-admin users**
4. **Hover/tap reveals detail — rest state is clean**
5. **Reader is full-screen and distraction-free**

---

## Typography

| Context | Font | Rationale |
|---|---|---|
| UI (all surfaces) | **Inter** | Neutral, legible, excellent screen rendering at all sizes |
| Reader body text | **Literata** | Designed by Google for e-reader screens; used by Google Play Books |
| Reader (user option) | Inter | For users who prefer sans-serif reading |
| Code / metadata | System monospace | Identifiers, ISBNs, file paths |

Both fonts are open source via Google Fonts. Loaded locally — no external font requests from a self-hosted app.

Reader font is a per-user preference stored in the DB. Default: Literata.

---

## Color System

Base: **shadcn/ui `zinc` palette** — neutral, works equally well in light and dark, allows cover art to pop without competing with the UI chrome.

### Light Mode
| Role | Value | Usage |
|---|---|---|
| Background | `zinc-50` | Page background |
| Surface | `white` | Cards, panels, modals |
| Border | `zinc-200` | Dividers, card borders |
| Text primary | `zinc-900` | Headings, body |
| Text secondary | `zinc-500` | Labels, metadata |
| Accent | `teal-600` | Buttons, links, active states, progress bars |
| Accent hover | `teal-700` | Button hover state |

### Dark Mode
| Role | Value | Usage |
|---|---|---|
| Background | `zinc-950` | Page background (not pure black) |
| Surface | `zinc-900` | Cards, panels, modals |
| Border | `zinc-800` | Dividers, card borders |
| Text primary | `zinc-50` | Headings, body |
| Text secondary | `zinc-400` | Labels, metadata |
| Accent | `teal-400` | Buttons, links, active states, progress bars |
| Accent hover | `teal-300` | Button hover state |

Theme is toggled in the user menu. Preference stored per user in DB. Defaults to system preference on first load.

---

## Layout — App Shell

```
┌─────────────────────────────────────────────────┐
│  ◉  Library Name              🔍        👤      │  Top bar
├──────┬──────────────────────────────────────────┤
│      │                                          │
│  📚  │                                          │
│  🔍  │          Main content area               │
│  📑  │                                          │
│  ⚙️  │                                          │
│      │                                          │
└──────┴──────────────────────────────────────────┘
```

### Top Bar
- **Left**: App logo + library name (configurable by admin)
- **Center**: Search input (expands on focus)
- **Right**: User avatar → dropdown (Profile, Theme toggle, Sign out, Admin Panel if admin role)
- Nothing else. No breadcrumbs, no notification bells, no feature shortcuts.

### Sidebar
- **Collapsed by default** — icons only, 48px wide
- **Expands on hover** (desktop) or tap (mobile) to show labels — 200px wide
- **Items**:
  - Library (cover grid)
  - Search
  - Shelves
  - Currently reading (shown only if reading progress exists)
- Sidebar never shows admin items — those live in the user menu
- On mobile: sidebar becomes a bottom tab bar (4 items max)

---

## Library View (Cover Grid)

Default landing page. One job: show your books.

### Grid
- Responsive columns: 2 (mobile) → 4 (tablet) → 6-8 (desktop)
- Uniform card height — covers cropped/padded to consistent aspect ratio (2:3)
- Lazy-loaded images with a `zinc-200` placeholder skeleton

### Book Card — Rest State
```
┌────────────┐
│            │
│   cover    │
│   art      │
│            │
│            │
├────────────┤
│ Title      │
│ Author     │
└────────────┘
```
Title + author only. No badges, no icons, no progress indicators at rest.

### Book Card — Hover/Tap State
Subtle dark overlay fades in over the cover:
- Reading progress bar (thin, bottom of cover — only if started)
- Two icon buttons: **Read** (open) and **Download**
- Nothing else

Tap the card body (not the buttons) → navigates to Book Detail.

### Toolbar (above grid)
- **Left**: Filter chips — Authors, Series, Tags, Language, Format (expandable pill row)
- **Right**: Sort dropdown (Title, Author, Date added, Rating) + Grid/List toggle
- Filters collapse to a single "Filters" button on mobile

### List View (alternate)
Triggered by grid/list toggle. Dense table: cover thumbnail, title, author, series, format badges, rating. Same hover actions. For power users who prefer scanning metadata over covers.

---

## Book Detail

Accessed by tapping a card. Slide-in or full navigation depending on screen size.

### Zone 1 — Hero
```
┌─────────────────────────────────────────────────┐
│  ←                                    •••       │
│                                                  │
│  [large cover]   Title                           │
│                  Author(s)                       │
│                  Series · Book 3                 │
│                                                  │
│                  ★★★★☆  (4/5)                   │
│                                                  │
│                  [ Read ]  [ Download ▾ ]        │
└─────────────────────────────────────────────────┘
```
- `←` back to library
- `•••` menu (admin/edit actions — only visible to users with `can_edit` or Admin role):
  - Edit metadata
  - Replace cover
  - Add format
  - Delete book
  - LLM: Classify, Validate, Quality check, Derive
- Download `▾` expands to list available formats

### Zone 2 — Metadata Strip
Horizontal scannable row:
```
  EN   ·   2024   ·   Fiction, Literary   ·   EPUB PDF
```
Language, year, tags (confirmed only), formats. Clicking a tag filters the library by that tag.

### Zone 3 — Expandable Sections
Collapsed by default. Each section has a chevron toggle.

| Section | Default | Contents |
|---|---|---|
| Description | Collapsed | Full book description |
| Formats | Collapsed | All formats with file size + download link |
| Identifiers | Collapsed | ISBN, ISBN-13, ASIN, etc. |
| Pending Tags | Collapsed (badge count if any) | LLM tag suggestions — confirm / reject UI |
| Series | Collapsed | Other books in series with mini-cards |
| Custom fields | Collapsed | Calibre custom columns (if any) |
| History | Collapsed (Admin only) | Audit log for this book |

---

## Reader

Full-screen. Entered by tapping Read on a card or book detail.

### Rest State
Pure content. No chrome.

### Active State (mouse move / tap edge)
Minimal toolbar fades in and auto-hides after 3 seconds:

```
┌─────────────────────────────────────────────────┐
│  ←  Title · Author                    ⚙️  ☰    │  ← fades in
├─────────────────────────────────────────────────┤
│                                                  │
│                                                  │
│              Book content                        │
│                                                  │
│                                                  │
├─────────────────────────────────────────────────┤
│  ▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░  42%                 │  ← thin progress bar
└─────────────────────────────────────────────────┘
```

- `←` exits reader, returns to book detail
- `⚙️` opens Reader Settings panel (slides in from right):
  - Font: Literata / Inter
  - Font size: slider
  - Line height: slider
  - Margin width: slider
  - Theme: Light / Sepia / Dark
- `☰` opens chapter/TOC panel (slides in from left)
- Progress bar: tap to scrub (epub by percentage, PDF by page)

### Reader Settings Panel
Slides in from right, does not block content. Dismisses on outside tap.

---

## Search

Accessed from top bar or sidebar.

### Inline (top bar)
Expands to a dropdown showing:
- Recent searches (last 5)
- Quick results — first 5 matching books as mini-cards (cover + title + author)
- "See all results →" link

### Search Page (full)
Results in the same cover grid layout as the library. Two tabs:
- **Library** — full-text search (Meilisearch)
- **Semantic** — AI similarity search (grayed out with tooltip if LLM unavailable)

Filter chips and sort available same as library view.

---

## Shelves

Personal reading lists. Accessed from sidebar.

- List of shelves with book count + cover mosaic (4 covers tiled)
- Tap shelf → same cover grid layout, filtered to that shelf
- Add to shelf: `•••` menu on any book card or book detail
- Create/rename/delete shelf: inline, no modal

---

## Admin Panel

Accessed via avatar → Admin Panel. Completely separate visual zone — full-page replacement, not a modal. Non-admin users never see this link.

Clean dashboard layout with sidebar navigation:

| Section | Contents |
|---|---|
| Dashboard | System stats card, LLM status, jobs summary, storage usage |
| Users | Table: username, role, active, last login. CRUD inline. |
| Roles | Role permission toggles — one row per role |
| Import | Bulk import UI — drag-drop zip or enter server path, dry run toggle, progress log |
| Migration | Calibre migrate — source path input, dry run, progress, history |
| Jobs | LLM job queue — filterable table, cancel pending jobs |
| Prompt Evals | Fixture list, run buttons, model × fixture matrix, promote prompt |
| System | Version, DB engine, storage stats, Meilisearch status |

Admin panel uses the same color system and typography — it does not look like a different app. Just a different section.

---

## LLM Features — Surface Points

LLM features are never in the main navigation. They appear contextually:

| Where | What |
|---|---|
| Book detail `•••` menu | Classify, Validate metadata, Quality check, Derive |
| Book detail Pending Tags section | Confirm / reject tag suggestions |
| Library toolbar | "AI: Classify selected" (bulk action, appears when books are selected) |
| Search page tabs | Semantic search tab (grayed out if unavailable) |
| Admin → Jobs | Monitor running classification jobs |
| Admin → Prompt Evals | Test and promote system prompts |

If LLM is disabled or unreachable, all LLM surfaces are **grayed out with a tooltip** explaining why — never hidden, never an error, never intrusive.

---

## Interaction Patterns

| Pattern | Usage |
|---|---|
| **Slide-in panels** | Reader settings, TOC, filter panel on mobile |
| **Expandable sections** | Book detail zones 3+ — collapsed by default |
| **Inline editing** | Shelf names, user rows in admin — no modal required for simple edits |
| **Modal dialogs** | Destructive confirmations only (delete book, delete user) |
| **Toast notifications** | Non-blocking feedback (book added, tag confirmed, import started) — bottom right, auto-dismiss |
| **Progress indicators** | Skeleton cards on load, thin top-of-page bar for navigation, spinner only inside the triggering element |
| **Empty states** | Illustrated, actionable — "No books yet. Import your library →" not just a blank grid |

---

## Mobile (Expo — Phase 2)

The mobile app uses the same design language. Key adaptations:

| Desktop | Mobile |
|---|---|
| Icon sidebar | Bottom tab bar (Library, Search, Shelves, Reading) |
| Hover states | Tap-and-hold for quick actions |
| Large cover grid | 2-column grid with larger tap targets |
| Slide-in panels | Bottom sheets |
| Top bar search | Dedicated search tab |
| Inline admin | Admin accessible from profile tab — same sections |

---

## Component Library

**shadcn/ui** — components are copied into the repo (`apps/web/components/ui/`), not a runtime dependency. Customized to match this design system.

Key components in use:
- `Card` — book cards, stat cards in admin
- `Sheet` — slide-in panels (reader settings, TOC, mobile filters)
- `Collapsible` — book detail expandable sections
- `DropdownMenu` — `•••` menus, user avatar menu, download format picker
- `Command` — search dropdown (inline quick results)
- `Tabs` — search (Library / Semantic), reader theme picker
- `Toast` — non-blocking feedback
- `Dialog` — destructive confirmations only
- `Skeleton` — loading states for cover grid

---

## Cover Placeholder

When a book has no cover art, a placeholder is generated from the title:

- Background: deterministic color derived from the title string (hashed to a muted teal/zinc palette — never garish)
- Content: first letter of the title, centered, in a large serif (Literata), light color on dark background
- Same 2:3 aspect ratio as real covers — grid stays uniform
- Generated client-side — no server round-trip

Example: "The Great Gatsby" → muted slate background + large "T"

---

## Reading Progress on Cards

A thin `teal-600` bar (3px) sits flush at the bottom of the cover image — visible at rest whenever reading progress exists for the current user. Cards with no progress started show no bar.

```
┌────────────┐
│            │
│   cover    │
│   art      │
│            │
│▓▓▓▓▓░░░░░░│  ← 3px teal progress bar (42% shown)
├────────────┤
│ Title      │
│ Author     │
└────────────┘
```

---

## Open Design Questions

All resolved. No open items.
