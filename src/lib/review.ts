import type { ReviewPreset } from "@/types";

export const INACTIVE_MONTHS = 12;
export const FORGOTTEN_STARRED_MONTHS = 24;
export const FORGOTTEN_PUSHED_MONTHS = 18;

export const SNOOZE_OPTIONS = [
  { days: 7, label: "1 week" },
  { days: 30, label: "1 month" },
  { days: 90, label: "3 months" },
  { days: 180, label: "6 months" },
] as const;

export type ReviewPresetMeta = {
  id: ReviewPreset;
  label: string;
  description: string;
  reason: string;
  /** History preset — excluded from active queue counts. */
  history: boolean;
};

export const REVIEW_PRESETS: ReviewPresetMeta[] = [
  {
    id: "uncategorized",
    label: "Uncategorized",
    description: "Active stars with no real category (or only Uncategorized).",
    reason: "Uncategorized",
    history: false,
  },
  {
    id: "archived",
    label: "Archived",
    description: "Archived on GitHub but still starred locally.",
    reason: "Archived, still starred",
    history: false,
  },
  {
    id: "inactive",
    label: "Inactive",
    description: `No push in ${INACTIVE_MONTHS}+ months (or never pushed).`,
    reason: "No push in 12+ months",
    history: false,
  },
  {
    id: "forgotten",
    label: "Possibly forgotten",
    description: `Starred ${FORGOTTEN_STARRED_MONTHS}+ months ago and no push in ${FORGOTTEN_PUSHED_MONTHS}+ months.`,
    reason: "Starred 2+ years · no push in 18+ months",
    history: false,
  },
  {
    id: "unstarred",
    label: "Unstarred history",
    description:
      "Previously starred, later unstarred. Local history only — not an active count.",
    reason: "Unstarred",
    history: true,
  },
];

export function reviewPresetMeta(id: ReviewPreset): ReviewPresetMeta {
  return REVIEW_PRESETS.find((p) => p.id === id) ?? REVIEW_PRESETS[0];
}

export function isYoungLibrary(
  activeStars: number,
  oldestStarredAt: string | null,
  inactive: number,
  forgotten: number,
): boolean {
  if (activeStars <= 0) {
    return false;
  }
  if (inactive > 0 || forgotten > 0) {
    return false;
  }
  if (!oldestStarredAt) {
    return true;
  }
  const then = Date.parse(oldestStarredAt);
  if (Number.isNaN(then)) {
    return true;
  }
  const months = (Date.now() - then) / (1000 * 60 * 60 * 24 * 30);
  return months < INACTIVE_MONTHS;
}
