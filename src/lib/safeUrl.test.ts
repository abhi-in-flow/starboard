import { describe, expect, test } from "bun:test";
import { isSafeMarkdownUrl, safeMarkdownUrl } from "./safeUrl";

describe("isSafeMarkdownUrl", () => {
  test("allows http/https and relative", () => {
    expect(isSafeMarkdownUrl("https://github.com/o/r")).toBe(true);
    expect(isSafeMarkdownUrl("http://192.168.1.10/docs")).toBe(true);
    expect(isSafeMarkdownUrl("/owner/repo#readme")).toBe(true);
    expect(isSafeMarkdownUrl("#section")).toBe(true);
    expect(isSafeMarkdownUrl("docs/guide.md")).toBe(true);
  });

  test("blocks dangerous schemes", () => {
    expect(isSafeMarkdownUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeMarkdownUrl("JAVASCRIPT:alert(1)")).toBe(false);
    expect(isSafeMarkdownUrl("data:text/html;base64,PHNjcmlwdD4=")).toBe(false);
    expect(isSafeMarkdownUrl("file:///etc/passwd")).toBe(false);
    expect(isSafeMarkdownUrl("vbscript:msgbox")).toBe(false);
    expect(isSafeMarkdownUrl("")).toBe(false);
    expect(safeMarkdownUrl("javascript:alert(1)")).toBeNull();
  });

  test("blocks embedded javascript and protocol-relative URLs", () => {
    expect(isSafeMarkdownUrl("http:javascript:alert(1)")).toBe(false);
    expect(
      isSafeMarkdownUrl("https://example.com/x?next=javascript:alert(1)"),
    ).toBe(false);
    expect(isSafeMarkdownUrl("//evil.example/payload")).toBe(false);
    expect(safeMarkdownUrl("//evil.example/payload")).toBeNull();
    expect(safeMarkdownUrl("https://github.com/o/r")).toBe(
      "https://github.com/o/r",
    );
  });
});
