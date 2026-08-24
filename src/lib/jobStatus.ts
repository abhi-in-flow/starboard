/** User-initiated cancel — show as muted info, never as a red failure. */
export function isCancelledMessage(
  message: string | null | undefined,
): boolean {
  if (!message) {
    return false;
  }
  return /cancell?ed/i.test(message);
}
