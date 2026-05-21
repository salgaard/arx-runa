// WebdriverIO configuration for Arx Runa e2e tests.
//
// tauri-driver is a Rust binary — there is no npm package for it.
//
// Prerequisites (one-time setup):
//   cargo install tauri-driver --locked
//   npm install                           (installs this file's devDependencies)
//   Windows only: install msedgedriver matching your Edge version
//     cargo install --git https://github.com/chippers/msedgedriver-tool
//     & "$HOME\.cargo\bin\msedgedriver-tool.exe"
//     Move-Item msedgedriver.exe "$HOME\.cargo\bin\msedgedriver.exe"
//
// Run (builds the app automatically on first run):
//   npm test                              (Windows / macOS)
//   xvfb-run npm test                     (Linux CI — needs a virtual display)
//
// Skip the build step if you ALREADY ran `cargo tauri build --debug --no-bundle`:
//   E2E_SKIP_BUILD=1 npm test
//
// IMPORTANT: do NOT set E2E_SKIP_BUILD=1 after a plain `cargo build` (the VS Code
// debug workflow).  That binary uses devUrl (http://localhost:1420) and shows a
// blank page when Trunk isn't serving.  Only `cargo tauri build` embeds the
// frontend and serves it from tauri://localhost.
//
// Use release binary (CI default via E2E_RELEASE=1):
//   E2E_RELEASE=1 npm test

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn, spawnSync } = require("child_process");
const { TEST_VAULT_NAME, LOADING_STATE_VAULT_NAME } = require("./helpers/vault");

const isWin = os.platform() === "win32";
const profile = process.env.E2E_RELEASE ? "release" : "debug";

const application = path.resolve(
  __dirname,
  `../../../target/${profile}/arx-runa-tauri${isWin ? ".exe" : ""}`,
);

const tauriDriverBin = path.resolve(
  os.homedir(),
  ".cargo",
  "bin",
  `tauri-driver${isWin ? ".exe" : ""}`,
);

// Root of the workspace (parent of src-tauri/tests/e2e).
const workspaceRoot = path.resolve(__dirname, "../../..");

let tauriDriver;
let shuttingDown = false;

function closeTauriDriver() {
  shuttingDown = true;
  tauriDriver?.kill();
}

// Removes vault directories created during the test run.
// Matches by name in vault-header.json so no UUID needs to be tracked.
function cleanupTestVaults() {
  const platform = os.platform();
  let dataDir;
  if (platform === "win32") {
    dataDir = process.env.APPDATA;
  } else if (platform === "darwin") {
    dataDir = path.join(os.homedir(), "Library", "Application Support");
  } else {
    dataDir =
      process.env.XDG_DATA_HOME ||
      path.join(os.homedir(), ".local", "share");
  }

  if (!dataDir) {
    console.warn("[e2e cleanup] Cannot determine data dir; skipping vault cleanup");
    return;
  }

  const vaultRoot = path.join(dataDir, "arx-runa", "vaults");
  if (!fs.existsSync(vaultRoot)) return;

  let removed = 0;
  for (const entry of fs.readdirSync(vaultRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const vaultDir = path.join(vaultRoot, entry.name);
    const headerPath = path.join(vaultDir, "vault-header.json");
    if (!fs.existsSync(headerPath)) continue;
    try {
      const header = JSON.parse(fs.readFileSync(headerPath, "utf8"));
      if (header.name === TEST_VAULT_NAME || header.name === LOADING_STATE_VAULT_NAME) {
        fs.rmSync(vaultDir, { recursive: true, force: true });
        console.log(`[e2e cleanup] Removed test vault: ${vaultDir}`);
        removed++;
      }
    } catch (err) {
      console.warn(
        `[e2e cleanup] Could not process ${headerPath}: ${err.message}`,
      );
    }
  }
  if (removed === 0) {
    console.log("[e2e cleanup] No test vaults found to remove");
  }
}

// Ensure tauri-driver is killed even if the process exits abnormally.
["exit", "SIGINT", "SIGTERM", "SIGHUP", "SIGBREAK"].forEach((sig) => {
  process.on(sig, () => {
    try {
      closeTauriDriver();
    } finally {
      process.exit();
    }
  });
});

exports.config = {
  host: "127.0.0.1",
  port: 4444,

  runner: "local",
  specs: ["./specs/**/*.spec.js"],
  maxInstances: 1,

  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": { application },
    },
  ],

  logLevel: "info",
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,

  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    // 120 s default — enough for a single Argon2 unlock (≤90 s on slow CI)
    // plus a lock cycle. Tests that chain multiple unlock/lock cycles override
    // via this.timeout() inside their describe block.
    timeout: 120000,
  },

  // Build the app before running tests (unless E2E_SKIP_BUILD=1).
  // Uses `cargo tauri build --debug --no-bundle` which runs Trunk to embed the
  // frontend WASM bundle into the binary. Plain `cargo build` is not sufficient
  // — without the embedded frontend the app crashes on startup.
  onPrepare: () => {
    // Remove vaults from any previous run that crashed before onComplete cleanup ran.
    cleanupTestVaults();

    if (process.env.E2E_SKIP_BUILD) {
      console.log("E2E_SKIP_BUILD set — skipping cargo tauri build");
      return;
    }
    const buildArgs = process.env.E2E_RELEASE
      ? ["tauri", "build", "--config", "src-tauri/tauri.conf.dev.json", "--no-bundle"]
      : ["tauri", "build", "--config", "src-tauri/tauri.conf.dev.json", "--debug", "--no-bundle"];
    console.log(`Building app: cargo ${buildArgs.join(" ")}`);
    const result = spawnSync("cargo", buildArgs, {
      cwd: workspaceRoot,
      stdio: "inherit",
      shell: isWin,
    });
    if (result.status !== 0) {
      console.error("cargo tauri build failed");
      process.exit(result.status ?? 1);
    }
  },

  // Start tauri-driver before the WebDriver session opens.
  beforeSession: () => {
    tauriDriver = spawn(tauriDriverBin, [], {
      stdio: [null, process.stdout, process.stderr],
    });
    tauriDriver.on("error", (err) => {
      console.error("tauri-driver error:", err);
      process.exit(1);
    });
    tauriDriver.on("exit", (code) => {
      if (!shuttingDown) {
        console.error("tauri-driver exited unexpectedly with code:", code);
        process.exit(1);
      }
    });
  },

  // Kill tauri-driver after the session ends.
  afterSession: () => {
    closeTauriDriver();
  },

  // Remove vaults created during the test run.
  onComplete: () => {
    cleanupTestVaults();
  },
};
