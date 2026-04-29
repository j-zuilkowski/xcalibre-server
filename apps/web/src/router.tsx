/**
 * TanStack Router route tree for xcalibre-server.
 *
 * Route hierarchy:
 *   rootRoute
 *   └── protectedRoute  (ProtectedRoute — redirects unauthenticated users to /login)
 *       ├── /           — redirect → /home
 *       ├── /home             — HomePage (dashboard: Continue Reading, Recently Added, Collections)
 *       ├── /library          — LibraryPage (legacy grid view, still accessible)
 *       ├── /browse/books     — BrowsePage (document_type=Book, alpha sidebar)
 *       ├── /browse/reference — BrowsePage (document_type=Reference)
 *       ├── /browse/periodicals — BrowsePage (document_type=Periodical)
 *       ├── /browse/magazines — BrowsePage (document_type=Magazine)
 *       ├── /downloads        — DownloadHistoryPage
 *       ├── /search           — SearchPage
 *       ├── /shelves          — ShelvesPage
 *       ├── /profile          — ProfilePage
 *       ├── /profile/stats    — StatsPage
 *       ├── /profile/import   — ImportPage (user)
 *       ├── /profile/webhooks — WebhooksPage
 *       ├── /books/$id        — BookDetailPage
 *       ├── /authors/$id      — AuthorPage
 *       ├── /books/$id/read/$format — ReaderPage
 *       └── /admin            — AdminLayout
 *           ├── /admin/dashboard
 *           ├── /admin/users
 *           ├── /admin/tags
 *           ├── /admin/authors
 *           ├── /admin/collections
 *           ├── /admin/import
 *           ├── /admin/jobs
 *           ├── /admin/scheduled-tasks
 *           ├── /admin/libraries
 *           ├── /admin/custom-columns
 *           ├── /admin/kobo-devices
 *           └── /admin/api-tokens
 *   ├── /login    — LoginPage  (public)
 *   └── /register — RegisterPage  (public, first-admin only)
 *
 * The router instance is exported as `router` and consumed by RouterProvider
 * in main.tsx.  The `routeTree` export is used for type-safe navigation.
 */
import { useEffect } from "react";
import { Outlet, createRootRoute, createRoute, createRouter, useNavigate } from "@tanstack/react-router";
import { LoginPage } from "./features/auth/LoginPage";
import { ProtectedRoute } from "./features/auth/ProtectedRoute";
import { RegisterPage } from "./features/auth/RegisterPage";
import { AdminLayout } from "./features/admin/AdminLayout";
import { DashboardPage } from "./features/admin/DashboardPage";
import { ImportPage as AdminImportPage } from "./features/admin/ImportPage";
import { AdminJobsPage } from "./features/admin/AdminJobsPage";
import { ApiTokensPage } from "./features/admin/ApiTokensPage";
import { KoboDevicesPage } from "./features/admin/KoboDevicesPage";
import { LibrariesPage } from "./features/admin/LibrariesPage";
import { CustomColumnsPage } from "./features/admin/CustomColumnsPage";
import { UsersPage } from "./features/admin/UsersPage";
import { ScheduledTasksPage } from "./features/admin/ScheduledTasksPage";
import { TagsPage } from "./features/admin/TagsPage";
import { AuthorsPage } from "./features/admin/AuthorsPage";
import { CollectionsPage } from "./features/admin/CollectionsPage";
import { ImportPage as ProfileImportPage } from "./features/profile/ImportPage";
import { ProfilePage } from "./features/profile/ProfilePage";
import { WebhooksPage } from "./features/profile/WebhooksPage";
import { StatsPage } from "./features/profile/StatsPage";
import { AuthorPage } from "./features/library/AuthorPage";
import { BookDetailPage } from "./features/library/BookDetailPage";
import { DownloadHistoryPage } from "./features/library/DownloadHistoryPage";
import { BrowsePage } from "./features/library/BrowsePage";
import { HomePage } from "./features/library/HomePage";
import { LibraryPage } from "./features/library/LibraryPage";
import { ShelvesPage } from "./features/library/ShelvesPage";
import { SearchPage } from "./features/search/SearchPage";
import { ReaderPage } from "./features/reader/ReaderPage";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const protectedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "protected",
  component: ProtectedRoute,
});

const libraryRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "library",
  component: LibraryPage,
});

const indexRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "/",
  component: () => {
    const navigate = useNavigate();

    useEffect(() => {
      void navigate({ to: "/home", replace: true });
    }, [navigate]);

    return null;
  },
});

const homeRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "home",
  component: HomePage,
});

const browseBookRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "browse/books",
  component: () => <BrowsePage documentType="Book" />,
});

const browseReferenceRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "browse/reference",
  component: () => <BrowsePage documentType="Reference" />,
});

const browsePeriodicalsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "browse/periodicals",
  component: () => <BrowsePage documentType="Periodical" />,
});

const browseMagazinesRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "browse/magazines",
  component: () => <BrowsePage documentType="Magazine" />,
});

const downloadHistoryRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "downloads",
  component: DownloadHistoryPage,
});

const searchRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "search",
  component: SearchPage,
});

const shelvesRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "shelves",
  component: ShelvesPage,
});

const profileRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "profile",
  component: ProfilePage,
});

const profileStatsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "profile/stats",
  component: StatsPage,
});

const profileImportRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "profile/import",
  component: ProfileImportPage,
});

const profileWebhooksRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "profile/webhooks",
  component: WebhooksPage,
});

const bookRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "books/$id",
  component: BookDetailPage,
});

const authorRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "authors/$id",
  component: AuthorPage,
});

const readerRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "books/$id/read/$format",
  component: ReaderPage,
});

const adminRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "admin",
  component: AdminLayout,
});

const adminDashboardRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "dashboard",
  component: DashboardPage,
});

const adminUsersRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "users",
  component: UsersPage,
});

const adminTagsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "tags",
  component: TagsPage,
});

const adminAuthorsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "authors",
  component: AuthorsPage,
});

const adminCollectionsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "collections",
  component: CollectionsPage,
});

const adminImportRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "import",
  component: AdminImportPage,
});

const adminJobsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "jobs",
  component: AdminJobsPage,
});

const adminTokensRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "tokens",
  component: ApiTokensPage,
});

const adminScheduledTasksRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "scheduled-tasks",
  component: ScheduledTasksPage,
});

const adminKoboDevicesRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "kobo-devices",
  component: KoboDevicesPage,
});

const adminLibrariesRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "libraries",
  component: LibrariesPage,
});

const adminCustomColumnsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "custom-columns",
  component: CustomColumnsPage,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "login",
  component: LoginPage,
});

const registerRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "register",
  component: RegisterPage,
});

export const routeTree = rootRoute.addChildren([
  protectedRoute.addChildren([
    libraryRoute,
    indexRoute,
    homeRoute,
    browseBookRoute,
    browseReferenceRoute,
    browsePeriodicalsRoute,
    browseMagazinesRoute,
    downloadHistoryRoute,
    searchRoute,
    shelvesRoute,
    profileRoute,
    profileStatsRoute,
    profileImportRoute,
    profileWebhooksRoute,
    bookRoute,
    authorRoute,
    readerRoute,
    adminRoute.addChildren([
      adminDashboardRoute,
      adminUsersRoute,
      adminTagsRoute,
      adminAuthorsRoute,
      adminCollectionsRoute,
      adminImportRoute,
      adminJobsRoute,
      adminTokensRoute,
      adminScheduledTasksRoute,
      adminLibrariesRoute,
      adminCustomColumnsRoute,
      adminKoboDevicesRoute,
    ]),
  ]),
  loginRoute,
  registerRoute,
]);

export const router = createRouter({
  routeTree,
});
