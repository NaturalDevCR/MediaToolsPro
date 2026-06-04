import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import BaseButton from "./BaseButton.vue";
import BaseField from "./BaseField.vue";
import BaseInput from "./BaseInput.vue";
import BaseProgressBar from "./BaseProgressBar.vue";
import BaseSegmentedControl from "./BaseSegmentedControl.vue";
import BaseToggle from "./BaseToggle.vue";

describe("UI primitives", () => {
  it("renders accessible field labels and input descriptions", () => {
    const wrapper = mount({
      components: { BaseField, BaseInput },
      template: `
        <BaseField id="url" label="Media URL" hint="Paste a source URL" v-slot="{ id, describedBy }">
          <BaseInput :id="id" :aria-describedby="describedBy" model-value="" />
        </BaseField>
      `,
    });

    expect(wrapper.get("label").attributes("for")).toBe("url");
    expect(wrapper.get("input").attributes("aria-describedby")).toBe("url-hint");
    expect(wrapper.text()).toContain("Media URL");
  });

  it("emits native-like control updates", async () => {
    const button = mount(BaseButton, { slots: { default: "Queue" } });
    expect(button.get("button").text()).toBe("Queue");

    const toggle = mount(BaseToggle, {
      props: { id: "auto", label: "Auto update", modelValue: false },
    });
    await toggle.get("input").setValue(true);
    expect(toggle.emitted("update:modelValue")?.[0]).toEqual([true]);

    const segmented = mount(BaseSegmentedControl, {
      props: {
        label: "Mode",
        modelValue: "audio",
        options: [
          { value: "audio", label: "Audio" },
          { value: "video", label: "Video" },
        ],
      },
    });
    await segmented.findAll("button")[1].trigger("click");
    expect(segmented.emitted("update:modelValue")?.[0]).toEqual(["video"]);
  });

  it("exposes progress semantics", () => {
    const wrapper = mount(BaseProgressBar, {
      props: { value: 35, label: "Download progress" },
    });
    const bar = wrapper.get('[role="progressbar"]');
    expect(bar.attributes("aria-label")).toBe("Download progress");
    expect(bar.attributes("aria-valuenow")).toBe("35");
  });
});
