// Zero-Trace e2e tests: verify the frontend leaves no sensitive traces in
// browser-managed storage or the DOM after the vault locks.
//
// These tests run against the real built app via tauri-driver + WebdriverIO.
// They complement the Rust-level source-scanning and runtime filesystem tests
// by exercising the WebView2 layer directly.

const assert = require("assert");
const {
  createAndUnlockVault,
  unlockExistingVault,
  lockVault,
} = require("../helpers/vault");

describe("Zero-Trace: no sensitive browser-side state after lock", function () {
  before(async function () {
    await createAndUnlockVault(browser);
  });

  it("localStorage is empty after lock", async function () {
    await lockVault(browser);
    const count = await browser.execute(
      () => Object.keys(localStorage).length,
    );
    assert.strictEqual(count, 0, "localStorage must be empty after lock");
  });

  it("sessionStorage is empty after lock", async function () {
    await unlockExistingVault(browser);
    await lockVault(browser);
    const count = await browser.execute(
      () => Object.keys(sessionStorage).length,
    );
    assert.strictEqual(count, 0, "sessionStorage must be empty after lock");
  });

  it("file list is cleared after lock", async function () {
    await unlockExistingVault(browser);

    // File list must be present while unlocked.
    const fileList = await browser.$('[data-testid="file-list"]');
    await fileList.waitForExist({
      timeout: 10000,
      timeoutMsg: "file-list must appear after unlock",
    });

    await lockVault(browser);

    // After lock, the vault browser is unmounted — file list must be gone.
    const fileListAfterLock = await browser.$('[data-testid="file-list"]');
    const exists = await fileListAfterLock.isExisting();
    assert.strictEqual(
      exists,
      false,
      "file list must not be present after lock",
    );
  });

  it("no vault UUID in URL after lock", async function () {
    await unlockExistingVault(browser);
    await lockVault(browser);

    const url = await browser.getUrl();
    const uuidPattern =
      /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;
    assert.ok(
      !uuidPattern.test(url),
      `URL must not contain a vault UUID after lock, got: ${url}`,
    );
  });
});
