// E2E tests for the arxvault:// video streaming scheme.
//
// These tests verify the Tauri IPC command returns a valid platform URL and
// that no <video> element leaks into the DOM when no video is actively playing.
//
// Full upload-then-play tests are not feasible here because the file-picker
// dialog cannot be automated via WebDriver.  Those flows are covered by the
// unit tests in src/ui/video_stream.rs and src/storage/pipeline/decrypt_file.rs.

const assert = require("assert");
const { createAndUnlockVault } = require("../helpers/vault");

describe("Video scheme", function () {
  before(async function () {
    await createAndUnlockVault(browser);
  });

  it("video_scheme_base_url IPC returns a platform-appropriate scheme URL", async function () {
    // __TAURI__.core is available because tauri.conf.json has withGlobalTauri: true.
    const baseUrl = await browser.execute(async () => {
      const { invoke } = window.__TAURI__.core;
      return await invoke("video_scheme_base_url");
    });

    // Windows WebView2 maps custom schemes to http://<scheme>.localhost.
    // macOS WKWebView and Linux WebKitGTK use <scheme>://localhost directly.
    const isValid =
      baseUrl === "http://arxvault.localhost" ||
      baseUrl === "arxvault://localhost";

    assert.ok(
      isValid,
      `video_scheme_base_url must return a known platform URL, got: ${baseUrl}`,
    );
  });

  it("no <video> element is present in the DOM when no video is playing", async function () {
    const count = await browser.execute(
      () => document.querySelectorAll("video").length,
    );
    assert.strictEqual(
      count,
      0,
      "<video> element must not be present when no video is playing",
    );
  });

  it("no arxvault:// or arxvault.localhost src leaks into the DOM at rest", async function () {
    // Verify the scheme URL does not appear in any attribute when no file is open.
    const found = await browser.execute(() => {
      const all = document.querySelectorAll("[src]");
      for (const el of all) {
        const src = el.getAttribute("src") || "";
        if (
          src.startsWith("arxvault://") ||
          src.includes("arxvault.localhost")
        ) {
          return src;
        }
      }
      return null;
    });

    assert.strictEqual(
      found,
      null,
      `No element should have an arxvault scheme src at rest, found: ${found}`,
    );
  });
});
