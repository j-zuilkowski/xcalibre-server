import { Outlet, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import React from "react";

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => <div />,
});

export const routeTree = rootRoute.addChildren([indexRoute]);

export const router = createRouter({
  routeTree,
});
