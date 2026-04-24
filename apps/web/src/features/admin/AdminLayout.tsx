import { useTranslation } from "react-i18next";
import { Link, Outlet, useLocation } from "@tanstack/react-router";

const NAV_ITEMS = [
  { to: "/admin/dashboard", label: "dashboard" },
  { to: "/admin/users", label: "users" },
  { to: "/admin/tags", label: "tags" },
  { to: "/admin/authors", label: "authors" },
  { to: "/admin/import", label: "import" },
  { to: "/admin/jobs", label: "jobs" },
  { to: "/admin/scheduled-tasks", label: "scheduled_tasks" },
  { to: "/admin/libraries", label: "libraries" },
  { to: "/admin/custom-columns", label: "custom_columns" },
  { to: "/admin/kobo-devices", label: "kobo_devices" },
];

export function AdminLayout() {
  const location = useLocation();
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 z-50 flex bg-zinc-950 text-zinc-100">
      <aside className="flex w-64 flex-col border-r border-zinc-800 bg-zinc-950">
        <div className="border-b border-zinc-800 px-5 py-5">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-teal-300">{t("nav.admin_panel")}</p>
          <h1 className="mt-2 text-xl font-semibold">{t("app_name")}</h1>
        </div>

        <nav aria-label="Admin navigation" className="flex flex-1 flex-col gap-1 px-3 py-4">
          {NAV_ITEMS.map((item) => {
            const active = location.pathname === item.to;
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`rounded-xl px-4 py-3 text-sm transition ${
                  active ? "bg-teal-500 text-zinc-950" : "text-zinc-300 hover:bg-zinc-900"
                }`}
              >
                {t(`admin.${item.label}`)}
              </Link>
            );
          })}
        </nav>
      </aside>

      <main className="min-w-0 flex-1 overflow-auto bg-[radial-gradient(circle_at_top_right,rgba(20,184,166,0.16),transparent_30%),linear-gradient(180deg,#0f172a_0%,#020617_100%)] px-6 py-6">
        <Outlet />
      </main>
    </div>
  );
}
