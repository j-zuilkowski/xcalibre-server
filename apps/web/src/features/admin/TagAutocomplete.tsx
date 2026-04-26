import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { TagLookupItem } from "@xs/shared";
import { apiClient } from "../../lib/api-client";

type TagAutocompleteProps = {
  onSelect: (tag: TagLookupItem) => void;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
};

export function TagAutocomplete({
  onSelect,
  placeholder = "Search tags",
  disabled = false,
  className = "",
}: TagAutocompleteProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  const trimmedQuery = query.trim();
  const tagsQuery = useQuery({
    queryKey: ["admin-tags", trimmedQuery],
    queryFn: () => apiClient.searchTags(trimmedQuery),
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

  const suggestions = useMemo(() => tagsQuery.data ?? [], [tagsQuery.data]);

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
        className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none placeholder:text-zinc-400 disabled:opacity-60"
      />

      {open ? (
        <div className="absolute left-0 right-0 top-[calc(100%+0.4rem)] z-50 rounded-xl border border-zinc-700 bg-zinc-950 shadow-2xl">
          {trimmedQuery.length === 0 ? (
            <p className="px-3 py-2 text-sm text-zinc-500">{t("admin.type_to_search_tags")}</p>
          ) : suggestions.length > 0 ? (
            <ul className="max-h-56 overflow-y-auto p-1">
              {suggestions.map((tag) => (
                <li key={tag.id}>
                  <button
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      onSelect(tag);
                      setQuery("");
                      setOpen(false);
                    }}
                    className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm text-zinc-100 hover:bg-zinc-800"
                  >
                    <span>{tag.name}</span>
                    <span className="text-xs text-zinc-500">{tag.id}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="px-3 py-2 text-sm text-zinc-500">
              {tagsQuery.isFetching ? t("common.searching") : t("admin.no_tags_found")}
            </p>
          )}
        </div>
      ) : null}
    </div>
  );
}
