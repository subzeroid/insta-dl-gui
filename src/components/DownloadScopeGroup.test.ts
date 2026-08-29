/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import DownloadScopeGroup from "./DownloadScopeGroup.vue";

function render(props: {
  shownCount: number;
  selectedCount: number;
  busy: boolean;
  allTitle?: string;
  helperText?: string;
  shownDisabledReason?: string;
  selectedDisabledReason?: string;
}) {
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

  it("renders custom All copy and visibly associates snapshot limits", () => {
    const wrapper = render({
      shownCount: 501,
      selectedCount: 501,
      busy: false,
      allTitle: "Fetch and download the complete Posts archive; uses API requests.",
      helperText: "Exact snapshots are limited to 500 items. Use All for a complete archive.",
      shownDisabledReason: "Shown has 501 items, above the 500-item limit.",
      selectedDisabledReason: "Selected has 501 items, above the 500-item limit.",
    });
    const buttons = wrapper.findAll("button");

    expect(buttons[0].attributes("title")).toBe(
      "Fetch and download the complete Posts archive; uses API requests.",
    );
    expect(buttons[0].attributes("disabled")).toBeUndefined();
    expect(buttons[1].attributes("disabled")).toBeDefined();
    expect(buttons[2].attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain(
      "Exact snapshots are limited to 500 items. Use All for a complete archive.",
    );
    expect(wrapper.text()).toContain("Shown has 501 items, above the 500-item limit.");
    expect(wrapper.text()).toContain("Selected has 501 items, above the 500-item limit.");
    for (const index of [1, 2]) {
      const describedBy = buttons[index]!.attributes("aria-describedby")!.split(" ");
      expect(describedBy.every((id) => wrapper.find(`#${id}`).exists())).toBe(true);
    }
  });

  it("evaluates Shown and Selected disabled reasons independently", () => {
    const shownBlocked = render({
      shownCount: 501,
      selectedCount: 1,
      busy: false,
      shownDisabledReason: "Shown limit exceeded.",
    }).findAll("button");
    expect(shownBlocked[0].attributes("disabled")).toBeUndefined();
    expect(shownBlocked[1].attributes("disabled")).toBeDefined();
    expect(shownBlocked[2].attributes("disabled")).toBeUndefined();

    const selectedBlocked = render({
      shownCount: 1,
      selectedCount: 501,
      busy: false,
      selectedDisabledReason: "Selected limit exceeded.",
    }).findAll("button");
    expect(selectedBlocked[0].attributes("disabled")).toBeUndefined();
    expect(selectedBlocked[1].attributes("disabled")).toBeUndefined();
    expect(selectedBlocked[2].attributes("disabled")).toBeDefined();
  });
});
