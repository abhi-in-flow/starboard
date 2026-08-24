export const ESCAPE_ORDER = [
  "Close the repository detail panel",
  "Clear the search query",
  "Clear all filters and review",
] as const;

export type EscapeAction =
  | "close-detail"
  | "clear-query"
  | "clear-filters"
  | "none";

export function nextEscapeAction(input: {
  detailOpen: boolean;
  hasQuery: boolean;
  hasFiltersOrReview: boolean;
}): EscapeAction {
  if (input.detailOpen) {
    return "close-detail";
  }
  if (input.hasQuery) {
    return "clear-query";
  }
  if (input.hasFiltersOrReview) {
    return "clear-filters";
  }
  return "none";
}

export const KEYBOARD_HELP =
  "/ focus search · ↑/↓ move selection · Esc closes detail, then search, then filters and review";

/** Roles that own ArrowUp/ArrowDown and `/` (Radix Select, menus, dialogs). */
export const LIBRARY_KEY_OWNER_ROLES = new Set([
  "combobox",
  "listbox",
  "option",
  "menu",
  "menuitem",
  "menuitemcheckbox",
  "menuitemradio",
  "dialog",
  "alertdialog",
  "tree",
  "treeitem",
  "grid",
  "gridcell",
  "slider",
  "spinbutton",
  "searchbox",
  "textbox",
]);

const OWNER_SLOTS = new Set([
  "select-trigger",
  "select-content",
  "dialog-content",
]);

export const LIBRARY_KEY_OWNER_SELECTOR = [
  "input",
  "textarea",
  "select",
  "[contenteditable]:not([contenteditable='false'])",
  ...[...LIBRARY_KEY_OWNER_ROLES].map((role) => `[role="${role}"]`),
  "[data-slot=select-trigger]",
  "[data-slot=select-content]",
  "[data-slot=dialog-content]",
].join(",");

export type KeyOwnerNode = {
  tagName: string;
  isContentEditable?: boolean;
  role?: string | null;
  dataSlot?: string | null;
  parent: KeyOwnerNode | null;
};

export function nodeOwnsLibraryKeys(node: KeyOwnerNode): boolean {
  if (node.isContentEditable) {
    return true;
  }
  const tag = node.tagName.toUpperCase();
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
    return true;
  }
  const role = (node.role ?? "").toLowerCase();
  if (LIBRARY_KEY_OWNER_ROLES.has(role)) {
    return true;
  }
  return OWNER_SLOTS.has((node.dataSlot ?? "").toLowerCase());
}

export function ancestorOwnsLibraryKeys(node: KeyOwnerNode | null): boolean {
  let current = node;
  while (current) {
    if (nodeOwnsLibraryKeys(current)) {
      return true;
    }
    current = current.parent;
  }
  return false;
}

/**
 * Window-bubble `/` and ArrowUp/Down must yield when a composite already
 * handled the event (`defaultPrevented`) or when the target sits in an
 * owning control. Role/slot/closest fallback covers Radix versions that
 * do not always preventDefault on the trigger.
 */
export function shouldHandleLibraryNavKeys(input: {
  defaultPrevented: boolean;
  target: KeyOwnerNode | null;
}): boolean {
  if (input.defaultPrevented) {
    return false;
  }
  return !ancestorOwnsLibraryKeys(input.target);
}

type KeyOwnerLike = {
  tagName: string;
  isContentEditable?: boolean;
  getAttribute?: (name: string) => string | null;
  closest?: (selector: string) => unknown;
  parentElement?: unknown;
};

function isKeyOwnerLike(value: unknown): value is KeyOwnerLike {
  return (
    value != null &&
    typeof value === "object" &&
    "tagName" in value &&
    typeof (value as { tagName: unknown }).tagName === "string"
  );
}

function likeToKeyOwner(el: KeyOwnerLike): KeyOwnerNode {
  return {
    tagName: el.tagName,
    isContentEditable: Boolean(el.isContentEditable),
    role: el.getAttribute?.("role") ?? null,
    dataSlot: el.getAttribute?.("data-slot") ?? null,
    parent: isKeyOwnerLike(el.parentElement)
      ? likeToKeyOwner(el.parentElement)
      : null,
  };
}

/** Prefer `closest` when available; otherwise walk the ancestor chain. */
export function keyOwnerFromEventTarget(
  target: EventTarget | null,
): KeyOwnerNode | null {
  if (!isKeyOwnerLike(target)) {
    return null;
  }
  if (typeof target.closest === "function") {
    const owner = target.closest(LIBRARY_KEY_OWNER_SELECTOR);
    if (isKeyOwnerLike(owner)) {
      return likeToKeyOwner(owner);
    }
  }
  return likeToKeyOwner(target);
}
