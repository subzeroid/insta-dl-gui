/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import DownloadScopeGroup from "./DownloadScopeGroup.vue";

function render(props: { shownCount: number; selectedCount: number; busy: boolean }) {
  return mount(DownloadScopeGroup, { props });
}

describe("DownloadScopeGroup", () => {
  it("keeps all three download scopes stable when nothing is shown or selected", () => {
    const wrapper = render({ shownCount: 0, selectedCount: 0, busy: false });
    const buttons = wrapper.findAll("button");

    expect(wrapper.text()).toContain("Download");
    expect(buttons.map((button) => button.text())).toEqual(["All", "Shown 0", "Selected 0"]);
    expect(buttons[0].attributes("disabled")).toBeUndefined();
    expect(buttons[1].attributes("disabled")).toBeDefined();
    expect(buttons[2].attributes("disabled")).toBeDefined();
  });

  it("emits the clicked scope once", async () => {
    const wrapper = render({ shownCount: 12, selectedCount: 3, busy: false });
    const buttons = wrapper.findAll("button");

    await buttons[0].trigger("click");
    await buttons[1].trigger("click");
    await buttons[2].trigger("click");

    expect(wrapper.emitted("download-all")).toEqual([[]]);
    expect(wrapper.emitted("download-shown")).toEqual([[]]);
    expect(wrapper.emitted("download-selected")).toEqual([[]]);
  });

  it("disables every scope while a download is busy", () => {
    const wrapper = render({ shownCount: 12, selectedCount: 3, busy: true });

    expect(wrapper.findAll("button").every((button) => button.attributes("disabled") !== undefined)).toBe(true);
  });

  it("exposes a named group and useful button titles", () => {
    const wrapper = render({ shownCount: 12, selectedCount: 3, busy: false });

    expect(wrapper.get('[role="group"]').attributes("aria-label")).toBe("Download");
    for (const button of wrapper.findAll("button")) {
      expect(button.attributes("title")).toBeTruthy();
    }
  });
});
