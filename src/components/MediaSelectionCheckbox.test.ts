/** @vitest-environment happy-dom */

import { defineComponent, ref } from "vue";
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import MediaSelectionCheckbox from "./MediaSelectionCheckbox.vue";

describe("MediaSelectionCheckbox", () => {
  it("reflects selection, labels its native checkbox, and emits once on change", async () => {
    const wrapper = mount(MediaSelectionCheckbox, {
      props: { selected: true, label: "Select post by @nike" },
    });
    const input = wrapper.get('input[type="checkbox"]');

    expect((input.element as HTMLInputElement).checked).toBe(true);
    expect(input.attributes("aria-label")).toBe("Select post by @nike");
    expect(wrapper.text()).toContain("✓");

    await input.trigger("change");

    expect(wrapper.emitted("toggle")).toEqual([[]]);
  });

  it("updates its checked and checkmark state from the selected prop", async () => {
    const wrapper = mount(MediaSelectionCheckbox, {
      props: { selected: false, label: "Select post" },
    });

    expect((wrapper.get('input[type="checkbox"]').element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.text()).not.toContain("✓");

    await wrapper.setProps({ selected: true });

    expect((wrapper.get('input[type="checkbox"]').element as HTMLInputElement).checked).toBe(true);
    expect(wrapper.text()).toContain("✓");
  });

  it("shows a visible focus treatment when its native input receives keyboard focus", async () => {
    const wrapper = mount(MediaSelectionCheckbox, {
      props: { selected: false, label: "Select post" },
    });

    await wrapper.get('input[type="checkbox"]').trigger("focus");

    expect(wrapper.get("label").classes()).toContain("focus-within:ring-2");
  });

  it("lets a parent control one native selection toggle without opening the card", async () => {
    const ParentCard = defineComponent({
      components: { MediaSelectionCheckbox },
      setup() {
        const selected = ref(false);
        const cardClicks = ref(0);
        const toggleCount = ref(0);
        function toggle() {
          toggleCount.value += 1;
          selected.value = !selected.value;
        }
        return { selected, cardClicks, toggleCount, toggle };
      },
      template: `
        <article @click="cardClicks += 1">
          <MediaSelectionCheckbox :selected="selected" label="Select post" @toggle="toggle" />
          <span data-testid="clicks">{{ cardClicks }}</span>
          <span data-testid="toggles">{{ toggleCount }}</span>
        </article>
      `,
    });
    const wrapper = mount(ParentCard);
    const input = wrapper.get('input[type="checkbox"]');

    expect((input.element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.text()).not.toContain("✓");

    await input.setValue(true);

    expect(wrapper.get('[data-testid="toggles"]').text()).toBe("1");
    expect(wrapper.get('[data-testid="clicks"]').text()).toBe("0");
    expect((input.element as HTMLInputElement).checked).toBe(true);
    expect(wrapper.text()).toContain("✓");
  });
});
