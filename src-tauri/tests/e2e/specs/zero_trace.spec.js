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
  // Each test does: beforeEach (waitUntil 90s + lockVault 20s) + test body
  // (unlockExistingVault 180s + lockVault 20s) = up to ~310 s worst-case on a
  // slow CI runner with cold Argon2. Override the global 120 s default.
  this.timeout(360000);

  before(async function () {
    await createAndUnlockVault(browser);
  });

  // Ensure each test starts from the vault picker regardless of where the
  // previous test left off. On slow CI runners a test may time out mid-unlock,
  // leaving the app in an indeterminate state and causing all subsequent tests
  // to fail via cascade. We wait for a known state (locked/unlocked/login), then
  // normalize to the vault picker so every test body starts from the same place.
  beforeEach(async function () {
    await browser.waitUntil(
      async () => {
        const lockBtn = await browser.$('[data-testid="lock-button"]');
        const vaultCard = await browser.$('[data-testid="vault-card"]');
        const passwordInput = await browser.$('[data-testid="password-input"]');
        return (
          (await lockBtn.isExisting()) ||
          (await vaultCard.isExisting()) ||
          (await passwordInput.isExisting())
        );
      },
      { timeout: 90000, timeoutMsg: "app stuck in unknown state before test" },
    );

    const lockBtn = await browser.$('[data-testid="lock-button"]');
    if (await lockBtn.isExisting()) {
      await lockVault(browser);
      return;
    }

    const passwordInput = await browser.$('[data-testid="password-input"]');
    if (await passwordInput.isExisting()) {
      const backBtn = await browser.$('//button[normalize-space()="← Back"]');
      await backBtn.waitForExist({ timeout: 10000 });
      await backBtn.click();
      const vaultCard = await browser.$('[data-testid="vault-card"]');
      await vaultCard.waitForExist({
        timeout: 20000,
        timeoutMsg: "vault picker did not appear after returning from unlock form",
      });
    }
  });

  it("localStorage is empty after lock", async function () {
    await unlockExistingVault(browser);
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
