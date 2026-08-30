/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import MediaTypeBadge from "./MediaTypeBadge.vue";

describe("MediaTypeBadge", () => {
  it.each([
    ["photo", 1, "PHOTO", "bg-sky-500/80"],
    ["video", 1, "VIDEO", "bg-rose-500/80"],
    ["carousel", 6, "CAROUSEL · 6", "bg-amber-500/80"],
    ["unknown", 0, "POST", "bg-slate-700/80"],
  ] as const)("renders the %s badge with accessible text and color", (kind, count, text, color) => {
    const wrapper = mount(MediaTypeBadge, { props: { kind, count } });
    const badge = wrapper.get('[role="img"]');

    expect(badge.text()).toBe(text);
    expect(badge.attributes("aria-label")).toBe(text);
    expect(badge.classes()).toContain(color);
    expect(badge.classes()).toContain("text-white");
  });
});
