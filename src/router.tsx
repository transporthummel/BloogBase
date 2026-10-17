import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { About } from "./about/About";
import { Discover } from "./discover/Discover";
import { Library } from "./library/Library";
import { ProfileRoute } from "./profile/ProfilePage";
import { Settings } from "./settings/Settings";
import { Setup } from "./setup/Setup";
import { Shell } from "./shell/Shell";

const rootRoute = createRootRoute({ component: Shell });

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Library,
});

const discoverRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/discover",
  component: Discover,
});

const profileRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/profiles/$profileId",
  component: ProfileRoute,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: Settings,
});

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/about",
  component: About,
});

const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup",
  component: Setup,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([
    libraryRoute,
    discoverRoute,
    profileRoute,
    settingsRoute,
    aboutRoute,
    setupRoute,
  ]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
