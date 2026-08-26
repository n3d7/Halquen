import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SafeMarkdown } from "./SafeMarkdown";

describe("SafeMarkdown", () => {
  it("drops raw HTML and renders links without clickable anchors", () => {
    const html = renderToStaticMarkup(
      <SafeMarkdown>{'<script>window.pwned = true</script>\n\n[documentation](https://example.invalid)'}</SafeMarkdown>,
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<a");
    expect(html).toContain("documentation");
    expect(html).toContain("safe-link");
  });
});
