import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AdminAuthor } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";

type AuthorAutocompleteProps = {
  onSelect: (author: AdminAuthor) => void;
  placeholder?: string;
  disabled?: boolean;
  excludeId?: string;
  className?: string;
};

export function AuthorAutocomplete({
  onSelect,
  placeholder = "Search authors",
  disabled = false,
  excludeId,
  className = "",
}: AuthorAutocompleteProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  const trimmedQuery = query.trim();
  const authorsQuery = useQuery({
    queryKey: ["admin-authors-autocomplete", trimmedQuery],
    queryFn: () => apiClient.listAdminAuthors({ q: trimmedQuery, page: 1, page_size: 10 }),
    enabled: open && trimmedQuery.length > 0 && !disabled,
  });

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }

    if (open) {
      window.addEventListener("pointerdown", onPointerDown);
    }

    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [open]);

  const suggestions = useMemo(
    () => (authorsQuery.data?.items ?? []).filter((author) => author.id !== excludeId),
    [authorsQuery.data?.items, excludeId],
  );

  return (
    <div ref={rootRef} className={`relative ${className}`}>
      <input
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        placeholder={placeholder}
        disabled={disabled}
        className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none placeholder:text-zinc-500 disabled:opacity-60"
      />

      {open ? (
        <div className="absolute left-0 right-0 top-[calc(100%+0.4rem)] z-50 rounded-xl border border-zinc-700 bg-zinc-950 shadow-2xl">
          {trimmedQuery.length === 0 ? (
            <p className="px-3 py-2 text-sm text-zinc-500">{t("common.search")}</p>
          ) : suggestions.length > 0 ? (
            <ul className="max-h-56 overflow-y-auto p-1">
              {suggestions.map((author) => (
                <li key={author.id}>
                  <button
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      onSelect(author);
                      setQuery("");
                      setOpen(false);
                    }}
                    className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm text-zinc-100 hover:bg-zinc-800"
                  >
                    <span className="min-w-0 truncate">{author.name}</span>
                    <span className="shrink-0 text-xs text-zinc-500">{author.book_count}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="px-3 py-2 text-sm text-zinc-500">
              {authorsQuery.isFetching ? t("common.searching") : t("common.none")}
            </p>
          )}
        </div>
      ) : null}
    </div>
  );
}
