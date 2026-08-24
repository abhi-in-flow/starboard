import { describe, expect, test } from "bun:test";
import {
  classifyJobFailure,
  errorMessage,
  isCancelledError,
  isCancelledMessage,
} from "./jobStatus";

describe("job cancellation classification", () => {
  test("detects cancelled and canceled messages", () => {
    expect(isCancelledMessage("operation cancelled")).toBe(true);
    expect(isCancelledMessage("Canceled by user")).toBe(true);
    expect(isCancelledMessage("README queue cancelled")).toBe(true);
    expect(isCancelledMessage("HTTP 503 unavailable")).toBe(false);
    expect(isCancelledMessage("")).toBe(false);
    expect(isCancelledMessage(null)).toBe(false);
  });

  test("detects Tauri cancelled errors by code or message", () => {
    expect(
      isCancelledError({ code: "cancelled", message: "operation cancelled" }),
    ).toBe(true);
    expect(
      isCancelledError({ code: "sync_error", message: "operation cancelled" }),
    ).toBe(true);
    expect(
      isCancelledError({
        code: "sync_error",
        message: "a sync is already running",
      }),
    ).toBe(false);
    expect(isCancelledError("Cancelled")).toBe(true);
  });

  test("classifies awaited start_sync cancel as informational", () => {
    const cancelled = classifyJobFailure({
      code: "cancelled",
      message: "operation cancelled",
    });
    expect(cancelled.kind).toBe("cancelled");
    expect(cancelled.message).toBe("operation cancelled");

    const failed = classifyJobFailure({
      code: "network_error",
      message: "HTTP 503: unavailable",
    });
    expect(failed.kind).toBe("error");
    expect(failed.message).toBe("HTTP 503: unavailable");
  });

  test("extracts messages from objects and Errors", () => {
    expect(errorMessage({ message: "boom" })).toBe("boom");
    expect(errorMessage(new Error("nope"))).toBe("nope");
    expect(errorMessage(undefined, "Sync failed")).toBe("Sync failed");
  });
});
