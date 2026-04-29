import { useState, type FormEvent } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { BookSummary, CollectionSummary } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { MediaCard } from "./MediaCard";

function RowHeader({
  title,
  seeAllHref = "/library",
}: {
  title: string;
  seeAllHref?: string;
}) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between gap-3">
      <h2 className="text-lg font-semibold tracking-tight text-zinc-900">{title}</h2>
      <a href={seeAllHref} className="text-sm font-medium text-teal-700 hover:underline">
        {t("home.see_all")} &gt;
      </a>
    </div>
  );
}

function BookTile({ book }: { book: BookSummary }) {
  return (
    <div className="w-32 shrink-0 md:w-40">
      <MediaCard book={book} progressPercentage={book.progress_percentage ?? 0} />
    </div>
  );
}

function CollectionTile({ collection }: { collection: CollectionSummary }) {
  return (
    <a
      href="/library"
      className="flex h-full w-32 shrink-0 flex-col justify-between rounded-2xl border border-zinc-200 bg-white p-4 text-left shadow-sm transition hover:border-teal-300 hover:shadow-md md:w-40"
    >
      <div className="space-y-2">
        <p className="line-clamp-3 text-sm font-semibold text-zinc-900">{collection.name}</p>
        {collection.description ? (
          <p className="line-clamp-4 text-xs leading-5 text-zinc-500">{collection.description}</p>
        ) : null}
      </div>
      <p className="mt-4 text-xs font-medium uppercase tracking-[0.16em] text-zinc-400">
        {collection.book_count} books
      </p>
    </a>
  );
}

export function HomePage() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [query, setQuery] = useState("");

  const continueReadingQuery = useQuery({
    queryKey: ["home", "continue-reading"],
    queryFn: () => apiClient.listInProgress(),
  });

  const recentlyAddedQuery = useQuery({
    queryKey: ["home", "recently-added"],
    queryFn: () =>
      apiClient.listBooks({
        sort: "created_at",
        order: "desc",
        page_size: 20,
      }),
  });

  const collectionsQuery = useQuery({
    queryKey: ["home", "collections"],
    queryFn: () => apiClient.listCollections(),
  });

  const continueReading = continueReadingQuery.data ?? [];
  const recentlyAdded = recentlyAddedQuery.data?.items ?? [];
  const collections = collectionsQuery.data ?? [];

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) {
      return;
    }

    void navigate({
      to: "/search",
      search: { q: trimmed },
    });
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,rgba(13,148,136,0.12),transparent_35%),linear-gradient(180deg,#fafafa_0%,#f4f7f6_100%)] px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-8">
        <section className="overflow-hidden rounded-3xl border border-teal-100 bg-white/85 p-6 shadow-sm backdrop-blur">
          <div className="max-w-3xl space-y-4">
            <div className="space-y-2">
              <p className="text-xs font-semibold uppercase tracking-[0.24em] text-teal-700">
                xcalibre
              </p>
              <h1 className="text-3xl font-semibold tracking-tight text-zinc-950 md:text-4xl">
                {t("nav.library")}
              </h1>
            </div>

            <form onSubmit={handleSubmit} className="w-full">
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("home.search_placeholder")}
                className="h-12 w-full rounded-full border border-zinc-200 bg-white px-4 text-sm text-zinc-900 shadow-sm outline-none transition placeholder:text-zinc-500 focus:border-teal-500 focus:ring-2 focus:ring-teal-100"
              />
            </form>
          </div>
        </section>

        {continueReading.length > 0 ? (
          <section className="space-y-4">
            <RowHeader title={t("home.continue_reading")} />
            <div className="flex gap-4 overflow-x-auto pb-2">
              {continueReading.map((book) => (
                <BookTile key={book.id} book={book} />
              ))}
            </div>
          </section>
        ) : null}

        <section className="space-y-4">
          <RowHeader title={t("home.recently_added")} />
          <div className="flex gap-4 overflow-x-auto pb-2">
            {recentlyAdded.map((book) => (
              <BookTile key={book.id} book={book} />
            ))}
          </div>
        </section>

        {collections.length > 0 ? (
          <section className="space-y-4">
            <RowHeader title={t("home.collections")} />
            <div className="flex gap-4 overflow-x-auto pb-2">
              {collections.map((collection) => (
                <CollectionTile key={collection.id} collection={collection} />
              ))}
            </div>
          </section>
        ) : null}
      </div>
    </main>
  );
}
