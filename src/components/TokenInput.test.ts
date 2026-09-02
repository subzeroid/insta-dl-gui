/** @vitest-environment happy-dom */

import { defineComponent, ref } from "vue";
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import TokenInput from "./TokenInput.vue";

describe("TokenInput", () => {
  it("toggles token visibility while preserving the model value", async () => {
    const Parent = defineComponent({
      components: { TokenInput },
      setup() {
        const token = ref("exact-draft-token");
        return { token };
      },
      template: `
        <TokenInput
          v-model="token"
          id="token-input"
          name="hiker-token"
          placeholder="Paste a token"
          autocomplete="off"
          class="font-mono"
        />
      `,
    });
    const wrapper = mount(Parent);
    const input = () => wrapper.get("input");
    const button = () => wrapper.get("button");

    expect(wrapper.get("div").classes()).toEqual(["relative", "min-w-0", "flex-1"]);
    expect(input().attributes("id")).toBe("token-input");
    expect(input().attributes("name")).toBe("hiker-token");
    expect(input().attributes("placeholder")).toBe("Paste a token");
    expect(input().attributes("autocomplete")).toBe("off");
    expect(input().classes()).toContain("font-mono");
    expect(input().attributes("type")).toBe("password");
    expect((input().element as HTMLInputElement).value).toBe("exact-draft-token");
    expect(button().attributes("type")).toBe("button");
    expect(button().attributes("aria-label")).toBe("Show token");
    expect(button().attributes("aria-pressed")).toBe("false");

    await button().trigger("click");

    expect(input().attributes("type")).toBe("text");
    expect((input().element as HTMLInputElement).value).toBe("exact-draft-token");
    expect(button().attributes("aria-label")).toBe("Hide token");
    expect(button().attributes("aria-pressed")).toBe("true");
    expect(wrapper.get("svg").attributes("aria-hidden")).toBe("true");

    await input().setValue("edited-draft-token");

    expect(wrapper.vm.token).toBe("edited-draft-token");

    await button().trigger("click");

    expect(input().attributes("type")).toBe("password");
    expect((input().element as HTMLInputElement).value).toBe("edited-draft-token");
  });

  it("disables both controls and keeps the token hidden", async () => {
    const wrapper = mount(TokenInput, {
      props: { modelValue: "stored-token", disabled: true },
    });

    expect((wrapper.get("input").element as HTMLInputElement).disabled).toBe(true);
    expect((wrapper.get("button").element as HTMLButtonElement).disabled).toBe(true);

    await wrapper.get("button").trigger("click");

    expect(wrapper.get("input").attributes("type")).toBe("password");
  });

  it("resets visibility when disabled or emptied before accepting a new token", async () => {
    const Parent = defineComponent({
      components: { TokenInput },
      setup() {
        const token = ref("initial-token");
        const disabled = ref(false);
        return { token, disabled };
      },
      template: '<TokenInput v-model="token" :disabled="disabled" />',
    });
    const wrapper = mount(Parent);
    const input = wrapper.get("input");
    const button = wrapper.get("button");

    await button.trigger("click");
    expect(input.attributes("type")).toBe("text");

    wrapper.vm.disabled = true;
    await wrapper.vm.$nextTick();
    expect(input.attributes("type")).toBe("password");
    expect(button.attributes("disabled")).toBeDefined();
    expect(button.attributes("aria-label")).toBe("Show token");
    expect(button.attributes("aria-pressed")).toBe("false");

    wrapper.vm.disabled = false;
    await wrapper.vm.$nextTick();
    expect(input.attributes("type")).toBe("password");

    await button.trigger("click");
    expect(input.attributes("type")).toBe("text");
    await input.setValue("");
    expect(input.attributes("type")).toBe("password");

    await input.setValue("new-token");
    expect(input.attributes("type")).toBe("password");
  });
});
