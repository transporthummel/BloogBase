import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Library } from "./library/Library";
import { Settings } from "./settings/Settings";
import { Setup } from "./setup/Setup";
import { Shell } from "./shell/Shell";

const rootRoute = createRootRoute({ component: Shell });

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Library,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: Settings,
});

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup",
  component: Setup,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([libraryRoute, settingsRoute, setupRoute]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
