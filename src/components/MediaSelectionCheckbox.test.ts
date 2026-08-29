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

  it("does not bubble checkbox clicks to the parent card", async () => {
    const ParentCard = defineComponent({
      components: { MediaSelectionCheckbox },
      setup() {
        const cardClicks = ref(0);
        return { cardClicks };
      },
      template: `
        <article @click="cardClicks += 1">
          <MediaSelectionCheckbox :selected="false" label="Select post" />
          <span data-testid="clicks">{{ cardClicks }}</span>
        </article>
      `,
    });
    const wrapper = mount(ParentCard);

    await wrapper.get('input[type="checkbox"]').trigger("click");
    await wrapper.get('input[type="checkbox"]').trigger("change");

    expect(wrapper.get('[data-testid="clicks"]').text()).toBe("0");
  });
});
