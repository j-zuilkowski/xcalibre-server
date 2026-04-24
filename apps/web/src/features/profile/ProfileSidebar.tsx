import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

type ProfileSidebarProps = {
  active: "profile" | "stats" | "import";
};

const NAV_ITEMS = [
  { key: "profile" as const, to: "/profile", label: "nav.profile" },
  { key: "stats" as const, to: "/profile/stats", label: "nav.reading_stats" },
  { key: "import" as const, to: "/profile/import", label: "nav.import_history" },
];

export function ProfileSidebar({ active }: ProfileSidebarProps) {
  const { t } = useTranslation();

  return (
    <aside className="lg:sticky lg:top-6 lg:w-56">
      <div className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-4 shadow-[0_24px_80px_-30px_rgba(15,118,110,0.45)]">
        <p className="text-xs font-semibold uppercase tracking-[0.24em] text-teal-300">{t("nav.profile")}</p>
        <div className="mt-4 space-y-2">
          {NAV_ITEMS.map((item) => {
            const isActive = active === item.key;
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex items-center justify-between rounded-2xl px-4 py-3 text-sm font-medium transition ${
                  isActive
                    ? "bg-teal-500/15 text-teal-200 ring-1 ring-teal-500/30"
                    : "text-zinc-300 hover:bg-zinc-800/80 hover:text-zinc-50"
                }`}
              >
                <span>{t(item.label)}</span>
                <span className="text-xs uppercase tracking-[0.24em] text-zinc-500">
                  {isActive ? "•" : ""}
                </span>
              </Link>
            );
          })}
        </div>
      </div>
    </aside>
  );
}
