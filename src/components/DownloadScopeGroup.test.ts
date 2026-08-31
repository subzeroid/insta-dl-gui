/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { nextTick } from "vue";

import DownloadScopeGroup from "./DownloadScopeGroup.vue";

const wrappers: Array<{ unmount: () => void }> = [];

function track<T extends { unmount: () => void }>(wrapper: T): T {
  wrappers.push(wrapper);
  return wrapper;
}

function render(props: {
  shownCount: number;
  selectedCount: number;
  busy: boolean;
  allTitle?: string;
  shownDisabledReason?: string;
  selectedDisabledReason?: string;
}) {
  return track(mount(DownloadScopeGroup, { props }));
}

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  document.body.replaceChildren();
});

describe("DownloadScopeGroup", () => {
  it("keeps all three download scopes stable when nothing is shown or selected", () => {
    const wrapper = render({ shownCount: 0, selectedCount: 0, busy: false });
    const buttons = wrapper.get('[role="group"]').findAll("button");

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

    expect(wrapper.get('[role="group"]').findAll("button").every((button) => button.attributes("disabled") !== undefined)).toBe(true);
  });

  it("exposes a named group and useful button titles", () => {
    const wrapper = render({ shownCount: 12, selectedCount: 3, busy: false });

    expect(wrapper.get('[role="group"]').attributes("aria-label")).toBe("Download");
    for (const button of wrapper.get('[role="group"]').findAll("button")) {
      expect(button.attributes("title")).toBeTruthy();
    }
  });

  it("toggles the scope help popover with an accessible relationship", async () => {
    const wrapper = render({ shownCount: 12, selectedCount: 3, busy: false });
    const help = wrapper.get('[data-action="scope-help"]');

    expect(help.attributes("aria-label")).toBe("Explain download scopes");
    expect(help.attributes("aria-expanded")).toBe("false");
    expect(wrapper.find(`#${help.attributes("aria-controls")}`).exists()).toBe(false);
    expect(wrapper.text()).not.toContain("complete category archive");

    await help.trigger("click");

    expect(help.attributes("aria-expanded")).toBe("true");
    const popover = wrapper.get(`#${help.attributes("aria-controls")}`);
    expect(popover.attributes("role")).toBe("dialog");
    expect(popover.text()).toContain("complete category archive");
    expect(popover.text()).toContain("may make API requests");
    expect(popover.text()).toContain("currently visible items");
    expect(popover.text()).toContain("Posts media filter");
    expect(popover.text()).toContain("including items hidden by the current filter");
    expect(popover.text()).toContain("limited to 500 items");

    await help.trigger("click");
    expect(help.attributes("aria-expanded")).toBe("false");
    expect(wrapper.find(`#${help.attributes("aria-controls")}`).exists()).toBe(false);
  });

  it("closes scope help on an outside click", async () => {
    const wrapper = track(mount(DownloadScopeGroup, {
      attachTo: document.body,
      props: { shownCount: 12, selectedCount: 3, busy: false },
    }));
    const help = wrapper.get('[data-action="scope-help"]');

    await help.trigger("click");
    document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    await nextTick();

    expect(help.attributes("aria-expanded")).toBe("false");
  });

  it("closes scope help with Escape from elsewhere in the document and returns focus to its trigger", async () => {
    const wrapper = track(mount(DownloadScopeGroup, {
      attachTo: document.body,
      props: { shownCount: 12, selectedCount: 3, busy: false },
    }));
    const help = wrapper.get('[data-action="scope-help"]');
    const outside = document.createElement("button");
    document.body.append(outside);

    await help.trigger("click");
    outside.focus();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await nextTick();

    expect(help.attributes("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(help.element);
  });

  it("keeps disabled snapshot reasons separate from the general help", async () => {
    const wrapper = render({
      shownCount: 501,
      selectedCount: 501,
      busy: false,
      allTitle: "Fetch and download the complete Posts archive; uses API requests.",
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
    expect(wrapper.text()).toContain("Shown has 501 items, above the 500-item limit.");
    expect(wrapper.text()).toContain("Selected has 501 items, above the 500-item limit.");
    for (const index of [1, 2]) {
      const describedBy = buttons[index]!.attributes("aria-describedby")!.split(" ");
      expect(describedBy.every((id) => wrapper.find(`#${id}`).exists())).toBe(true);
    }

    await wrapper.get('[data-action="scope-help"]').trigger("click");
    expect(wrapper.text()).toContain("limited to 500 items");
    expect(wrapper.text()).toContain("Shown has 501 items, above the 500-item limit.");
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
