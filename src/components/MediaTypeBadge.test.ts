/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import MediaTypeBadge from "./MediaTypeBadge.vue";

describe("MediaTypeBadge", () => {
  it.each([
    ["photo", 1, "PHOTO", "bg-sky-700"],
    ["video", 1, "VIDEO", "bg-rose-700"],
    ["carousel", 6, "CAROUSEL · 6", "bg-amber-700"],
    ["unknown", 0, "POST", "bg-slate-700"],
  ] as const)("renders the %s badge with accessible text and color", (kind, count, text, color) => {
    const wrapper = mount(MediaTypeBadge, { props: { kind, count } });
    const badge = wrapper.get('[role="img"]');

    expect(badge.text()).toBe(text);
    expect(badge.attributes("aria-label")).toBe(text);
    expect(badge.classes()).toContain(color);
    expect(badge.classes()).toContain("text-white");
  });

  it.each([undefined, 0, 1, 1.5, -1])(
    "does not render a carousel label for invalid count %s",
    (count) => {
      const wrapper = mount(MediaTypeBadge, { props: { kind: "carousel", count } as any });

      expect(wrapper.text()).toBe("POST");
      expect(wrapper.text()).not.toContain("CAROUSEL");
    },
  );
});
