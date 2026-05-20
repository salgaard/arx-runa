// File operations e2e tests: verify the vault file browser behaves correctly
// for upload-related UI state and adversarial authentication scenarios.
//
// Note: actual encrypt/decrypt round-trip correctness is covered by the Rust
// integration tests in tests/integration_cloud_sync.rs. These tests focus on
// the UI layer — what the user sees — rather than the cryptographic pipeline.

const assert = require("assert");
const {
  createAndUnlockVault,
  unlockExistingVault,
  lockVault,
  TEST_VAULT_NAME,
} = require("../helpers/vault");

describe("File operations: vault file browser state", function () {
  before(async function () {
    await createAndUnlockVault(browser);
  });

  beforeEach(async function () {
    // Restore to a known state: vault unlocked, file browser visible.
    await browser.waitUntil(
      async () => {
        const lockBtn = await browser.$('[data-testid="lock-button"]');
        const vaultCard = await browser.$('[data-testid="vault-card"]');
        return (await lockBtn.isExisting()) || (await vaultCard.isExisting());
      },
      { timeout: 45000, timeoutMsg: "app stuck in unknown state before test" },
    );

    const vaultCard = await browser.$('[data-testid="vault-card"]');
    if (await vaultCard.isExisting()) {
      await unlockExistingVault(browser);
    }
  });

  it("file list element is present when vault is unlocked", async function () {
    const fileList = await browser.$('[data-testid="file-list"]');
    await fileList.waitForExist({
      timeout: 10000,
      timeoutMsg: "file-list must appear after unlock",
    });

    const exists = await fileList.isExisting();
    assert.strictEqual(
      exists,
      true,
      "file-list must be present in the DOM while vault is unlocked",
    );
  });

  it("file list element is absent when vault is locked", async function () {
    await lockVault(browser);

    const fileList = await browser.$('[data-testid="file-list"]');
    const exists = await fileList.isExisting();
    assert.strictEqual(
      exists,
      false,
      "file-list must not be present after lock",
    );
  });

  after(async function () {
    try {
      await lockVault(browser);
    } catch (_) {
      // Best-effort; already locked is fine.
    }
  });
});

describe("File operations: adversarial authentication", function () {
  // This suite requires a pre-existing vault created by the previous suite.
  // If that suite is skipped the vault-card may not be present; the before
  // hook guards against this by checking for both picker and app states.
  before(async function () {
    // Ensure we are at the vault picker (locked state).
    await browser.waitUntil(
      async () => {
        const vaultCard = await browser.$('[data-testid="vault-card"]');
        const lockBtn = await browser.$('[data-testid="lock-button"]');
        return (await vaultCard.isExisting()) || (await lockBtn.isExisting());
      },
      { timeout: 45000, timeoutMsg: "app not in a known state before adversarial suite" },
    );

    const lockBtn = await browser.$('[data-testid="lock-button"]');
    if (await lockBtn.isExisting()) {
      await lockVault(browser);
    }
  });

  it("wrong password does not unlock the vault", async function () {
    const vaultCard = await browser.$('[data-testid="vault-card"]');
    await vaultCard.waitForExist({ timeout: 15000 });
    await vaultCard.click();

    const passwordInput = await browser.$('[data-testid="password-input"]');
    await passwordInput.waitForExist({ timeout: 10000 });
    await passwordInput.setValue("this-is-definitely-the-wrong-password-8675309");

    const submitBtn = await browser.$('[data-testid="login-submit"]');
    await submitBtn.click();

    // The lock-button only appears after a successful unlock. Wait long enough
    // for the Argon2id KDF to run and the IPC round-trip to complete, then
    // assert the vault remains locked.
    await browser.pause(5000);

    const lockBtn = await browser.$('[data-testid="lock-button"]');
    const isUnlocked = await lockBtn.isExisting();
    assert.strictEqual(
      isUnlocked,
      false,
      "vault must NOT be unlocked when wrong password is submitted",
    );

    // The password form must still be visible — the UI stays on the unlock page.
    const formStillPresent = await passwordInput.isExisting();
    assert.strictEqual(
      formStillPresent,
      true,
      "password input must still be visible after failed unlock attempt",
    );
  });

  after(async function () {
    // Navigate back to the picker in case the test left the app mid-flow.
    try {
      const lockBtn = await browser.$('[data-testid="lock-button"]');
      if (await lockBtn.isExisting()) {
        await lockVault(browser);
      }
    } catch (_) {
      // Best-effort.
    }
  });
});
