# Codex Desktop App — Capabilities Reference

Source: https://developers.openai.com/codex/app  
Updated: April 2026

---

## What Codex Can Do

### Computer Use (macOS only)
Codex can see the entire desktop and interact with any app using its own cursor — clicking, typing, navigating menus, reading screen content, and taking screenshots.

**Setup (one-time):**
1. Codex Settings → Computer Use → click **Install**
2. Grant macOS **Screen Recording** permission (so Codex can see the app)
3. Grant macOS **Accessibility** permission (so Codex can click and type)

**How to invoke in a prompt:**
- Mention `@Computer Use` or `@AppName` (e.g. `@Safari`, `@Simulator`)
- Or just ask directly: "Launch the dev server and visually inspect the library page"

**What it's for:**
- Testing desktop or iOS Simulator apps
- Reproducing GUI-only bugs
- Verifying frontend UI after code changes
- Clicking through flows a browser test can't reach

**Limitations:**
- Cannot automate terminal apps or Codex itself
- macOS only (not available in EEA/UK/Switzerland)

---

### In-App Browser
Codex has a built-in browser that can render local dev servers and file-backed pages. When the **Browser plugin** is installed, Codex can operate it directly.

**Open it:**
- Toolbar button, click any URL, or `Cmd+Shift+B`

**What it can do (with Browser plugin):**
- Navigate to a local dev server (e.g. http://localhost:5173)
- Click and type into the rendered UI
- Inspect rendered page state
- Capture screenshots
- Verify that a fix works visually

**What it cannot do:**
- Authenticated/signed-in pages
- Existing browser profiles, cookies, or extensions

**How to invoke in a prompt:**
- "Open http://localhost:5173 in the browser and verify the library grid renders correctly"
- "Navigate to /books/1 and confirm the collapsible sections work"

---

### Background Computer Use
Multiple Codex agents can run in the background simultaneously without interrupting foreground work. Codex sends a notification when done.

---

### Terminal
Integrated terminal scoped to the current project. Toggle with `Cmd+J`. Supports running scripts, git operations, cargo, pnpm, etc.

---

### Git Integration
- Built-in diff pane with inline comments
- Commit, push, and PR creation from the interface
- Worktree support for isolated parallel work

---

### Memory
Codex can remember preferences and context between tasks. Can be configured in settings.

---

### Execution Modes
| Mode | What it does |
|---|---|
| Local | Works directly in your project directory |
| Worktree | Isolated git-based changes (safe parallel work) |
| Cloud | Remote execution |

---

## How to Use Visual Inspection in Phase Prompts

To have Codex visually inspect the UI after building a feature, add a step like this to the phase prompt:

```
After all tests pass:
1. Run: pnpm --filter @xs/web dev &
2. @Computer Use — open http://localhost:5173 in the in-app browser
3. Navigate through the golden path: login → library grid → book detail → reader
4. Screenshot any visual regressions
5. Fix what you see before committing
```

Or for a more targeted check:

```
@Browser — open http://localhost:5173/library and confirm:
- Cover grid renders with book cards
- Hover states show the action buttons
- Empty state shows when no books are present
Kill the dev server when done.
```

---

## Phase File Convention

Every phase that builds UI should include a visual inspection block after the TDD loop:

```
VISUAL INSPECTION (after all unit tests pass):
  pnpm --filter @xs/web dev &
  @Computer Use — open the app at http://localhost:5173
  Walk through: [list the key routes for this phase]
  Fix any visual regressions before committing.
  Kill the dev server.
```
