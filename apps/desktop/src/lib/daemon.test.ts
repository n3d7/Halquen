import { describe, expect, it } from "vitest";
import { commandMessage } from "./daemon";

describe("daemon error presentation", () => {
  it("uses a bounded sanitized message", () => {
    expect(commandMessage({ code: "provider_unavailable", message: "Provider unavailable" })).toBe("Provider unavailable");
    expect(commandMessage({ code: "internal", message: "x".repeat(600) })).toBe(
      "Halquen couldn't complete this operation.",
    );
    expect(commandMessage(new TypeError("internal JavaScript detail"))).toBe(
      "Halquen couldn't complete this operation.",
    );
  });

  it("does not stringify arbitrary error payloads", () => {
    expect(commandMessage({ api_key: "TEST_SECRET" })).not.toContain("TEST_SECRET");
  });
});
