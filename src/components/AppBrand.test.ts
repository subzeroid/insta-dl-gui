/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import AppBrand from "./AppBrand.vue";

describe("AppBrand", () => {
  it("renders a normalized version below the compact product name", () => {
    const wrapper = mount(AppBrand, { props: { version: " 0.6.0 " } });

    expect(wrapper.element.tagName).toBe("SPAN");
    expect(wrapper.attributes()).toHaveProperty("data-app-brand");
    expect(wrapper.classes()).toEqual(expect.arrayContaining(["inline-flex", "flex-col", "items-start"]));
    expect(wrapper.classes()).not.toContain("select-none");
    expect(wrapper.get("[data-app-name]").text()).toBe("insta-dl-gui");
    expect(wrapper.get("[data-app-name]").classes()).toEqual(expect.arrayContaining(["text-lg", "select-none"]));
    expect(wrapper.get("[data-app-version]").text()).toBe("v0.6.0");
    expect(wrapper.get("[data-app-version]").classes()).toContain("text-[10px]");
    expect(wrapper.get("[data-app-version]").classes()).not.toContain("text-xs");
    expect(wrapper.element.children[0].hasAttribute("data-app-name")).toBe(true);
    expect(wrapper.element.children[1].hasAttribute("data-app-version")).toBe(true);
  });

  it.each([undefined, null, "", "   "])("does not reserve a version row for unavailable version %j", (version) => {
    const wrapper = mount(AppBrand, { props: { version } });

    expect(wrapper.find("[data-app-version]").exists()).toBe(false);
    expect(wrapper.element.children).toHaveLength(1);
  });

  it("uses centered large styling when requested", () => {
    const wrapper = mount(AppBrand, {
      props: { version: "0.6.0", size: "large", align: "center" },
    });

    expect(wrapper.classes()).toEqual(expect.arrayContaining(["items-center", "text-center"]));
    expect(wrapper.get("[data-app-name]").classes()).toContain("text-2xl");
    expect(wrapper.get("[data-app-version]").classes()).toContain("text-[10px]");
    expect(wrapper.get("[data-app-version]").classes()).not.toContain("text-xs");
  });
});
