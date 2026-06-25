// Shared helpers for creating, unlocking, and locking a vault via the UI.
//
// Vault creation always uses Tier 1 (password only) to avoid the key-file
// file-picker dialog, which cannot be automated via WebDriver.

const TEST_VAULT_PASSWORD = "e2e-test-password-arx-runa-2026";
const TEST_VAULT_NAME = "e2e-test-vault";
const LOADING_STATE_VAULT_NAME = "loading-state-test-vault";

/**
 * Dismisses the demo-warning modal that opens on every app launch.
 *
 * The modal owns a RAM-only signal, so it only appears once per app launch
 * (the whole suite shares a single tauri-driver session). Its z-50 overlay
 * intercepts clicks until dismissed. This is a no-op for later suites in the
 * same session, once the modal has already been closed.
 */
async function dismissDemoWarning(browser) {
  const dismiss = await browser.$('[data-testid="demo-warning-dismiss"]');
  if (await dismiss.isExisting()) {
    await dismiss.click();
    await dismiss.waitForExist({ reverse: true, timeout: 5000 });
  }
}

/**
 * Creates a new vault via the UI (Tier 1, password only) and leaves the
 * session unlocked. Expects the VaultPicker to be visible on entry.
 */
async function createAndUnlockVault(browser) {
  // Wait for the WebView to navigate away from about:blank to tauri://localhost.
  // tauri-driver connects immediately on process start, before WebView2 has
  // loaded the app page, so a fixed sleep is not reliable.
  await browser.waitUntil(
    async () => {
      const url = await browser.getUrl();
      return url && url !== "about:blank";
    },
    { timeout: 30000, timeoutMsg: "App URL never left about:blank" },
  );

  // Log diagnostics so failures are easy to diagnose.
  const url = await browser.getUrl();
  const title = await browser.getTitle();
  const readyState = await browser.execute(() => document.readyState);
  console.log("[e2e] url:", url, "| title:", title, "| readyState:", readyState);

  // Wait for the DOM to be fully loaded and Leptos/WASM to render.
  await browser.waitUntil(
    async () => {
      const rs = await browser.execute(() => document.readyState);
      return rs === "complete";
    },
    { timeout: 30000, timeoutMsg: "document never reached readyState=complete" },
  );

  // Wait for Leptos/WASM to mount at least one child into <body>.
  await browser.waitUntil(
    async () => {
      const count = await browser.execute(
        () => document.body.childElementCount,
      );
      return count > 0;
    },
    { timeout: 30000, timeoutMsg: "Leptos WASM never mounted into <body>" },
  );

  const bodyHtml = await browser.execute(() => document.body.innerHTML);
  console.log("[e2e] body snapshot (first 2000 chars):", bodyHtml.slice(0, 2000));

  const createBtn = await browser.$('[data-testid="create-vault-button"]');
  await createBtn.waitForExist({ timeout: 15000 });

  // The demo-warning modal overlays the picker on first launch; dismiss it
  // before the click so it isn't intercepted.
  await dismissDemoWarning(browser);

  await createBtn.click();

  // Fill vault name.
  const nameInput = await browser.$('[data-testid="vault-name-input"]');
  await nameInput.waitForExist({ timeout: 5000 });
  await nameInput.setValue(TEST_VAULT_NAME);

  // Fill password.
  const passwordInput = await browser.$('[data-testid="create-password-input"]');
  await passwordInput.setValue(TEST_VAULT_PASSWORD);

  // Switch to Tier 1 (password only) — Tier 2 requires a key file dialog.
  const tierSelect = await browser.$('[data-testid="tier-select"]');
  await tierSelect.selectByAttribute("value", "1");

  // Submit.
  const submitBtn = await browser.$('[data-testid="create-vault-submit"]');
  await submitBtn.click();

  // After creation a "Set up recovery phrase?" modal appears for all tiers.
  // Dismiss it with "Remind Me Later" so the session can complete unlocking.
  const remindLater = await browser.$('[data-testid="recovery-remind-later"]');
  await remindLater.waitForExist({ timeout: 15000 });
  await remindLater.click();

  // Wait for the lock button to appear, confirming the vault is unlocked.
  await browser.waitUntil(
    async () => {
      const lockBtn = await browser.$('[data-testid="lock-button"]');
      return lockBtn.isExisting();
    },
    { timeout: 20000, timeoutMsg: "Vault did not unlock within 20s after creation" },
  );
}

/**
 * Unlocks an existing vault. Expects a vault card to be visible in the picker.
 */
async function unlockExistingVault(browser) {
  // CI runners may have multiple vault cards from previous suites. Select by
  // known test vault name so we don't submit this password to the wrong vault.
  await browser.waitUntil(
    async () => {
      const cards = await browser.$$('[data-testid="vault-card"]');
      return cards.length > 0;
    },
    { timeout: 20000, timeoutMsg: "No vault cards found in picker" },
  );
  const vaultCards = await browser.$$('[data-testid="vault-card"]');
  let targetCard = null;
  for (const card of vaultCards) {
    const text = await card.getText();
    if (text.includes(TEST_VAULT_NAME)) {
      targetCard = card;
      break;
    }
  }
  if (!targetCard) {
    throw new Error(`Test vault card not found: ${TEST_VAULT_NAME}`);
  }
  await targetCard.click();

  const passwordInput = await browser.$('[data-testid="password-input"]');
  await passwordInput.waitForExist({ timeout: 10000 });
  await passwordInput.setValue(TEST_VAULT_PASSWORD);

  const submitBtn = await browser.$('[data-testid="login-submit"]');
  await submitBtn.click();

  await browser.waitUntil(
    async () => {
      const lockBtn = await browser.$('[data-testid="lock-button"]');
      return lockBtn.isExisting();
    },
    { timeout: 180000, timeoutMsg: "Vault did not unlock within 180s" },
  );
}

/**
 * Locks the vault. Expects the lock button to be visible on entry.
 * Waits for the vault picker to reappear before returning.
 */
async function lockVault(browser) {
  const lockBtn = await browser.$('[data-testid="lock-button"]');
  await lockBtn.waitForExist({ timeout: 5000 });
  await lockBtn.click();

  await browser.waitUntil(
    async () => {
      const card = await browser.$('[data-testid="vault-card"]');
      const createBtn = await browser.$('[data-testid="create-vault-button"]');
      return (await card.isExisting()) || (await createBtn.isExisting());
    },
    { timeout: 20000, timeoutMsg: "Vault did not lock within 20s" },
  );
}

module.exports = { TEST_VAULT_NAME, LOADING_STATE_VAULT_NAME, createAndUnlockVault, unlockExistingVault, lockVault, dismissDemoWarning };
