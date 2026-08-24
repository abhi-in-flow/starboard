/** User-initiated cancel — show as muted info, never as a red failure. */
export function isCancelledMessage(
  message: string | null | undefined,
): boolean {
  if (!message) {
    return false;
  }
  return /cancell?ed/i.test(message);
}

export function errorMessage(
  err: unknown,
  fallback = "Request failed",
): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  if (err instanceof Error) {
    return err.message;
  }
  if (typeof err === "string" && err.trim()) {
    return err;
  }
  return fallback;
}

/** True for Tauri `{ code: "cancelled" }` or a cancelled/canceled message. */
export function isCancelledError(err: unknown): boolean {
  if (err && typeof err === "object" && "code" in err) {
    if (String((err as { code: unknown }).code) === "cancelled") {
      return true;
    }
  }
  return isCancelledMessage(errorMessage(err, ""));
}

export type JobFailureKind = "cancelled" | "error";

export function classifyJobFailure(
  err: unknown,
  fallback = "Request failed",
): {
  kind: JobFailureKind;
  message: string;
} {
  return {
    kind: isCancelledError(err) ? "cancelled" : "error",
    message: errorMessage(err, fallback),
  };
}
