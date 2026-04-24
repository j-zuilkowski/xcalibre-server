import { Outlet, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { LoginPage } from "./features/auth/LoginPage";
import { ProtectedRoute } from "./features/auth/ProtectedRoute";
import { RegisterPage } from "./features/auth/RegisterPage";
import { AdminLayout } from "./features/admin/AdminLayout";
import { DashboardPage } from "./features/admin/DashboardPage";
import { ImportPage } from "./features/admin/ImportPage";
import { AdminJobsPage } from "./features/admin/AdminJobsPage";
import { KoboDevicesPage } from "./features/admin/KoboDevicesPage";
import { LibrariesPage } from "./features/admin/LibrariesPage";
import { CustomColumnsPage } from "./features/admin/CustomColumnsPage";
import { UsersPage } from "./features/admin/UsersPage";
import { ScheduledTasksPage } from "./features/admin/ScheduledTasksPage";
import { TagsPage } from "./features/admin/TagsPage";
import { ProfilePage } from "./features/profile/ProfilePage";
import { StatsPage } from "./features/profile/StatsPage";
import { BookDetailPage } from "./features/library/BookDetailPage";
import { DownloadHistoryPage } from "./features/library/DownloadHistoryPage";
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

const bookRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "books/$id",
  component: BookDetailPage,
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

const adminImportRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "import",
  component: ImportPage,
});

const adminJobsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: "jobs",
  component: AdminJobsPage,
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
    downloadHistoryRoute,
    searchRoute,
    shelvesRoute,
    profileRoute,
    profileStatsRoute,
    bookRoute,
    readerRoute,
    adminRoute.addChildren([
      adminDashboardRoute,
      adminUsersRoute,
      adminTagsRoute,
      adminImportRoute,
      adminJobsRoute,
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
