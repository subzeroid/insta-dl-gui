/** @vitest-environment happy-dom */

import { describe, expect, it } from "vitest";
import { router } from "./router";

describe("application router", () => {
  it("redirects the root route to Explore", () => {
    const rootRoute = router.getRoutes().find((route) => route.path === "/");

    expect(rootRoute?.redirect).toBe("/explore");
  });
});
