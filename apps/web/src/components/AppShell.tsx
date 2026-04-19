import { useEffect, useRef, useState } from "react";
import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import { SearchBar } from "../features/search/SearchBar";
import { useAuthStore } from "../lib/auth-store";

type ThemeMode = "light" | "sepia" | "dark";

const THEME_STORAGE_KEY = "calibre-web.theme";

function readTheme(): ThemeMode {
  if (
    typeof localStorage === "undefined" ||
    typeof localStorage.getItem !== "function"
  ) {
    return "light";
  }

  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  if (stored === "sepia" || stored === "dark" || stored === "light") {
    return stored;
  }

  return "light";
}

function persistTheme(theme: ThemeMode): void {
  if (
    typeof localStorage === "undefined" ||
    typeof localStorage.setItem !== "function"
  ) {
    return;
  }

  localStorage.setItem(THEME_STORAGE_KEY, theme);
}

function nextTheme(current: ThemeMode): ThemeMode {
  if (current === "light") {
    return "sepia";
  }
  if (current === "sepia") {
    return "dark";
  }
  return "light";
}

function isAdmin(roleName: string | undefined): boolean {
  return roleName?.toLowerCase() === "admin";
}

export function AppShell() {
  const navigate = useNavigate();
  const user = useAuthStore((state) => state.user);
  const clearAuth = useAuthStore((state) => state.clearAuth);
  const [theme, setTheme] = useState<ThemeMode>(() => readTheme());
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const initial = user?.username?.trim()[0]?.toUpperCase() ?? "A";

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    persistTheme(theme);
  }, [theme]);

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }

    if (menuOpen) {
      window.addEventListener("pointerdown", onPointerDown);
    }

    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [menuOpen]);

  function signOut() {
    clearAuth();
    void navigate({ to: "/login", replace: true });
  }

  return (
    <div className={`min-h-screen ${theme === "dark" ? "bg-zinc-950 text-zinc-100" : "bg-zinc-50 text-zinc-900"}`}>
      <aside className="group fixed left-0 top-0 z-40 flex h-full w-12 flex-col border-r border-zinc-200 bg-white/95 shadow-sm transition-[width] duration-200 hover:w-48">
        <div className="flex h-16 items-center justify-center border-b border-zinc-200">
          <span className="rounded-lg bg-teal-600 px-2 py-1 text-xs font-bold uppercase tracking-[0.2em] text-white">
            cw
          </span>
        </div>

        <nav className="flex flex-1 flex-col gap-2 px-2 py-3 text-sm">
          {[
            { to: "/library", label: "Library", icon: "L" },
            { to: "/search", label: "Search", icon: "S" },
            { to: "/shelves", label: "Shelves", icon: "H" },
          ].map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="flex items-center gap-3 rounded-xl px-2 py-2 text-zinc-700 transition hover:bg-zinc-100 hover:text-zinc-900"
              activeProps={{ className: "bg-teal-50 text-teal-700" }}
            >
              <span className="grid h-7 w-7 place-items-center rounded-lg bg-zinc-900 text-xs font-bold text-white">
                {item.icon}
              </span>
              <span className="whitespace-nowrap opacity-0 transition-opacity duration-200 group-hover:opacity-100">
                {item.label}
              </span>
            </Link>
          ))}
        </nav>
      </aside>

      <header className="fixed left-12 right-0 top-0 z-30 h-16 border-b border-zinc-200 bg-white/95 backdrop-blur">
        <div className="flex h-full items-center gap-4 px-4">
          <Link to="/library" className="text-sm font-semibold tracking-wide text-zinc-900">
            calibre-web
          </Link>

          <div className="flex-1">
            <SearchBar />
          </div>

          <div className="relative" ref={menuRef}>
            <button
              type="button"
              onClick={() => setMenuOpen((open) => !open)}
              className="flex h-10 w-10 items-center justify-center rounded-full border border-zinc-300 bg-zinc-100 text-sm font-semibold text-zinc-800"
              aria-label="User menu"
            >
              {initial}
            </button>

            {menuOpen ? (
              <div className="absolute right-0 mt-2 w-56 overflow-hidden rounded-2xl border border-zinc-200 bg-white shadow-2xl">
                <a
                  href="/library"
                  className="block px-4 py-3 text-sm text-zinc-700 hover:bg-zinc-100"
                  onClick={() => setMenuOpen(false)}
                >
                  Profile
                </a>
                <button
                  type="button"
                  onClick={() => {
                    setTheme((current) => nextTheme(current));
                    setMenuOpen(false);
                  }}
                  className="block w-full px-4 py-3 text-left text-sm text-zinc-700 hover:bg-zinc-100"
                >
                  Theme: {theme}
                </button>
                {isAdmin(user?.role.name) ? (
                  <Link
                    to="/admin/dashboard"
                    className="block px-4 py-3 text-sm text-zinc-700 hover:bg-zinc-100"
                    onClick={() => setMenuOpen(false)}
                  >
                    Admin Panel
                  </Link>
                ) : null}
                <button
                  type="button"
                  onClick={signOut}
                  className="block w-full px-4 py-3 text-left text-sm text-red-600 hover:bg-red-50"
                >
                  Sign out
                </button>
              </div>
            ) : null}
          </div>
        </div>
      </header>

      <main className="min-h-screen pl-12 pt-16">
        <Outlet />
      </main>
    </div>
  );
}
