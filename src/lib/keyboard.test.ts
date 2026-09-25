import { describe, expect, test } from "bun:test";
import {
  ancestorOwnsLibraryKeys,
  type KeyOwnerNode,
  keyOwnerFromEventTarget,
  nextEscapeAction,
  nodeOwnsLibraryKeys,
  shouldHandleLibraryNavKeys,
} from "./keyboard";

function node(
  partial: Omit<KeyOwnerNode, "parent"> & { parent?: KeyOwnerNode | null },
): KeyOwnerNode {
  return { parent: partial.parent ?? null, ...partial };
}

describe("nextEscapeAction", () => {
  test("order is detail → query → filters/review", () => {
    expect(
      nextEscapeAction({
        detailOpen: true,
        hasQuery: true,
        hasFiltersOrReview: true,
      }),
    ).toBe("close-detail");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: true,
        hasFiltersOrReview: true,
      }),
    ).toBe("clear-query");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: false,
        hasFiltersOrReview: true,
      }),
    ).toBe("clear-filters");
    expect(
      nextEscapeAction({
        detailOpen: false,
        hasQuery: false,
        hasFiltersOrReview: false,
      }),
    ).toBe("none");
  });
});

describe("shouldHandleLibraryNavKeys", () => {
  test("yields when the event is already defaultPrevented", () => {
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: true,
        target: node({ tagName: "BODY" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: true,
        target: node({ tagName: "BUTTON" }),
      }),
    ).toBe(false);
  });

  test("yields for native form controls and contentEditable", () => {
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "INPUT" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "TEXTAREA" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "SELECT" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "DIV", isContentEditable: true }),
      }),
    ).toBe(false);
  });

  test("yields for Radix roles on the target", () => {
    for (const role of [
      "combobox",
      "listbox",
      "option",
      "menu",
      "menuitem",
    ] as const) {
      expect(
        shouldHandleLibraryNavKeys({
          defaultPrevented: false,
          target: node({ tagName: "DIV", role }),
        }),
      ).toBe(false);
    }
  });

  test("yields when an ancestor is a Radix listbox, combobox, or menu", () => {
    const option = node({
      tagName: "DIV",
      role: "option",
      parent: node({
        tagName: "DIV",
        role: "listbox",
        parent: node({ tagName: "DIV" }),
      }),
    });
    expect(ancestorOwnsLibraryKeys(option)).toBe(true);
    expect(
      shouldHandleLibraryNavKeys({ defaultPrevented: false, target: option }),
    ).toBe(false);

    const inTrigger = node({
      tagName: "SPAN",
      parent: node({ tagName: "BUTTON", role: "combobox" }),
    });
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: inTrigger,
      }),
    ).toBe(false);

    const inMenu = node({
      tagName: "SPAN",
      parent: node({
        tagName: "DIV",
        role: "menuitem",
        parent: node({ tagName: "DIV", role: "menu" }),
      }),
    });
    expect(
      shouldHandleLibraryNavKeys({ defaultPrevented: false, target: inMenu }),
    ).toBe(false);
  });

  test("yields for dialog and select data-slot fallbacks", () => {
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "DIV", role: "dialog" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({
          tagName: "SPAN",
          parent: node({ tagName: "DIV", role: "dialog" }),
        }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "BUTTON", dataSlot: "select-trigger" }),
      }),
    ).toBe(false);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({
          tagName: "DIV",
          dataSlot: "select-content",
          parent: node({ tagName: "DIV" }),
        }),
      }),
    ).toBe(false);
  });

  test("handles body, chrome, and repo selection controls", () => {
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "BODY" }),
      }),
    ).toBe(true);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({ tagName: "BUTTON" }),
      }),
    ).toBe(true);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: node({
          tagName: "BUTTON",
          parent: node({ tagName: "LI" }),
        }),
      }),
    ).toBe(true);
    expect(
      shouldHandleLibraryNavKeys({
        defaultPrevented: false,
        target: null,
      }),
    ).toBe(true);
  });

  test("nodeOwnsLibraryKeys does not treat generic buttons as owners", () => {
    expect(nodeOwnsLibraryKeys(node({ tagName: "BUTTON" }))).toBe(false);
    expect(
      nodeOwnsLibraryKeys(node({ tagName: "BUTTON", role: "combobox" })),
    ).toBe(true);
  });
});

describe("keyOwnerFromEventTarget", () => {
  test("uses closest when an ancestor matches a Radix role selector", () => {
    const option = {
      tagName: "DIV",
      getAttribute: (name: string) => (name === "role" ? "option" : null),
      closest: (selector: string) =>
        selector.includes('[role="option"]') ? option : null,
    };
    const owner = keyOwnerFromEventTarget(option as unknown as EventTarget);
    expect(owner?.role).toBe("option");
    expect(
      shouldHandleLibraryNavKeys({ defaultPrevented: false, target: owner }),
    ).toBe(false);
  });

  test("walks ancestors when closest is unavailable", () => {
    const listbox = {
      tagName: "DIV",
      getAttribute: (name: string) => (name === "role" ? "listbox" : null),
    };
    const label = {
      tagName: "SPAN",
      getAttribute: () => null,
      parentElement: listbox,
    };
    const owner = keyOwnerFromEventTarget(label as unknown as EventTarget);
    expect(
      shouldHandleLibraryNavKeys({ defaultPrevented: false, target: owner }),
    ).toBe(false);
  });
});
