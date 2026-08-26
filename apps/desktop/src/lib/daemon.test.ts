import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { commandMessage, daemon } from "./daemon";

beforeEach(() => invoke.mockReset());

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

describe("daemon chat lifecycle bridge", () => {
  it("uses the caller-owned request id for chat and cancellation", async () => {
    invoke.mockResolvedValueOnce({}).mockResolvedValueOnce(true);
    const input = {
      session_id: null,
      message: "hello",
      model_selection: { kind: "automatic" as const },
    };

    await daemon.sendChat("request:chat:test", input);
    await daemon.cancelChat("request:chat:test");

    expect(invoke).toHaveBeenNthCalledWith(1, "send_chat_message", {
      requestId: "request:chat:test",
      input,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "cancel_chat_message", {
      requestId: "request:chat:test",
    });
  });

  it("exposes historical log cleanup as a typed command", async () => {
    invoke.mockResolvedValueOnce(2);
    await expect(daemon.clearOperationalLogs()).resolves.toBe(2);
    expect(invoke).toHaveBeenCalledWith("clear_operational_logs");
  });
});
