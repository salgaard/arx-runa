// Loading-states e2e tests: verify that long-running operations show progress
// feedback and clean up correctly when they complete.
//
// These tests run against the real built app via tauri-driver + WebdriverIO.
// They complement the Rust-level source-scanning in security_audit.rs by
// exercising the WebView2 layer directly.

const assert = require("assert");
const { LOADING_STATE_VAULT_NAME, createAndUnlockVault, lockVault } = require("../helpers/vault");

describe("Loading states: vault creation", function () {
  // Each test in this suite operates on a fresh vault creation flow, so we
  // only wait for the app to load rather than pre-creating a vault.
  before(async function () {
    await browser.waitUntil(
      async () => {
        const url = await browser.getUrl();
        return url && url !== "about:blank";
      },
      { timeout: 30000, timeoutMsg: "App URL never left about:blank" },
    );

    await browser.waitUntil(
      async () => {
        const rs = await browser.execute(() => document.readyState);
        return rs === "complete";
      },
      { timeout: 30000, timeoutMsg: "document never reached readyState=complete" },
    );

    await browser.waitUntil(
      async () => {
        const count = await browser.execute(
          () => document.body.childElementCount,
        );
        return count > 0;
      },
      { timeout: 30000, timeoutMsg: "Leptos WASM never mounted into <body>" },
    );
  });

  it("submit button is disabled immediately after clicking create vault", async function () {
    const createBtn = await browser.$('[data-testid="create-vault-button"]');
    await createBtn.waitForExist({ timeout: 15000 });
    await createBtn.click();

    const nameInput = await browser.$('[data-testid="vault-name-input"]');
    await nameInput.waitForExist({ timeout: 5000 });
    await nameInput.setValue(LOADING_STATE_VAULT_NAME);

    const passwordInput = await browser.$('[data-testid="create-password-input"]');
    await passwordInput.setValue("loading-state-test-pw-2026");

    const tierSelect = await browser.$('[data-testid="tier-select"]');
    await tierSelect.selectByAttribute("value", "1");

    const submitBtn = await browser.$('[data-testid="create-vault-submit"]');
    await submitBtn.click();

    // The button must be disabled while the async IPC call is in-flight.
    // We check immediately after click — the `loading` signal is set
    // synchronously before the spawn_local resolves.
    const disabled = await submitBtn.getAttribute("disabled");
    assert.ok(
      disabled !== null,
      "create-vault submit button must be disabled while vault creation is in progress",
    );
  });

  it("no progress modal lingers after vault creation completes", async function () {
    // Dismiss the recovery-phrase modal that appears after creation.
    const remindLater = await browser.$('[data-testid="recovery-remind-later"]');
    await remindLater.waitForExist({
      timeout: 20000,
      timeoutMsg: "recovery-phrase modal never appeared after vault creation",
    });
    await remindLater.click();

    // Wait for the vault to finish unlocking.
    await browser.waitUntil(
      async () => {
        const lockBtn = await browser.$('[data-testid="lock-button"]');
        return lockBtn.isExisting();
      },
      { timeout: 20000, timeoutMsg: "Vault did not unlock within 20s after creation" },
    );

    // The ProgressModal must have closed automatically at 100% — its
    // data-testid element must not be present in the DOM.
    const modal = await browser.$('[data-testid="progress-modal"]');
    const exists = await modal.isExisting();
    assert.strictEqual(
      exists,
      false,
      "progress-modal must not be present in the DOM after vault creation completes",
    );
  });

  it("sync button is disabled while syncing", async function () {
    // Vault is unlocked from the previous test. Trigger a sync and check the
    // button state transitions correctly.
    const syncBtn = await browser.$('[data-testid="sync-button"]');
    await syncBtn.waitForExist({ timeout: 10000 });

    // Only check if there is something to sync or if sync is available.
    const isDisabledBefore = await syncBtn.getAttribute("disabled");
    if (isDisabledBefore !== null) {
      // Button already disabled (e.g., already syncing from a prior op) — skip.
      return;
    }

    await syncBtn.click();

    // The button must be disabled while the sync IPC is in-flight.  In CI the
    // backend can return almost instantly (no real destination), so the window
    // is narrow — poll for up to 1 s rather than doing a single bare read.
    await browser.waitUntil(
      async () => {
        const d = await syncBtn.getAttribute("disabled");
        return d !== null;
      },
      {
        timeout: 1000,
        timeoutMsg: "sync button must be disabled while a sync is in-flight",
      },
    );

    // Wait for sync to complete (button re-enables).
    await browser.waitUntil(
      async () => {
        const d = await syncBtn.getAttribute("disabled");
        return d === null;
      },
      { timeout: 30000, timeoutMsg: "Sync did not complete within 30s" },
    );
  });

  after(async function () {
    // Clean up: lock the vault so the next spec suite starts from the picker.
    try {
      await lockVault(browser);
    } catch (_) {
      // Best-effort; if the vault is already locked this is a no-op.
    }
  });
});
