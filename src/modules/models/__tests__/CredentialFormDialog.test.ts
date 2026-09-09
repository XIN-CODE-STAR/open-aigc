import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import CredentialFormDialog from "../components/CredentialFormDialog.vue";

describe("CredentialFormDialog", () => {
  it("renders grid when open=true and editing=null", () => {
    const wrapper = mount(CredentialFormDialog, {
      props: { open: true, editing: null, busy: false },
      attachTo: document.body,
    });
    // The ModalDialog teleports to body, so check document.body
    const backdrop = document.body.querySelector(".dialog-backdrop");
    expect(backdrop).not.toBeNull();
    const grid = document.body.querySelector(".preset-grid");
    expect(grid).not.toBeNull();
    const cards = document.body.querySelectorAll(".preset-card");
    expect(cards.length).toBe(20);
    wrapper.unmount();
  });

  it("does not render when open=false", () => {
    const wrapper = mount(CredentialFormDialog, {
      props: { open: false, editing: null, busy: false },
      attachTo: document.body,
    });
    const backdrop = document.body.querySelector(".dialog-backdrop");
    expect(backdrop).toBeNull();
    wrapper.unmount();
  });

  it("emits close when cancel clicked", async () => {
    const wrapper = mount(CredentialFormDialog, {
      props: { open: true, editing: null, busy: false },
      attachTo: document.body,
    });
    const cancelBtn = document.body.querySelector(".btn--secondary");
    expect(cancelBtn).not.toBeNull();
    await cancelBtn?.dispatchEvent(new MouseEvent("click"));
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });
});
