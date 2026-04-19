import { Outlet, createRootRoute, createRoute, createRouter, redirect } from "@tanstack/react-router";
import { LoginPage } from "./features/auth/LoginPage";
import { ProtectedRoute } from "./features/auth/ProtectedRoute";
import { RegisterPage } from "./features/auth/RegisterPage";
import { LibraryPage } from "./features/library/LibraryPage";

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
  component: () => (
    <div style={{ padding: "32px", fontFamily: "Inter, system-ui, sans-serif" }}>
      Book detail
    </div>
  ),
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
  protectedRoute.addChildren([indexRoute, libraryRoute, bookRoute]),
  loginRoute,
  registerRoute,
]);

export const router = createRouter({
  routeTree,
});
