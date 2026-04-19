import { Outlet, createRootRoute, createRoute, createRouter, redirect } from "@tanstack/react-router";
import { LoginPage } from "./features/auth/LoginPage";
import { ProtectedRoute } from "./features/auth/ProtectedRoute";
import { RegisterPage } from "./features/auth/RegisterPage";
import { BookDetailPage } from "./features/library/BookDetailPage";
import { LibraryPage } from "./features/library/LibraryPage";
import { ReaderPage } from "./features/reader/ReaderPage";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const protectedRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: ProtectedRoute,
});

const indexRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/library", replace: true });
  },
  component: () => null,
});

const libraryRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: "library",
  component: LibraryPage,
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
  protectedRoute.addChildren([indexRoute, libraryRoute, bookRoute, readerRoute]),
  loginRoute,
  registerRoute,
]);

export const router = createRouter({
  routeTree,
});
