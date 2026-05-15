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

const os = require("os");
const path = require("path");
const { spawn, spawnSync } = require("child_process");

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
    timeout: 60000,
  },

  // Build the app before running tests (unless E2E_SKIP_BUILD=1).
  // Uses `cargo tauri build --debug --no-bundle` which runs Trunk to embed the
  // frontend WASM bundle into the binary. Plain `cargo build` is not sufficient
  // — without the embedded frontend the app crashes on startup.
  onPrepare: () => {
    if (process.env.E2E_SKIP_BUILD) {
      console.log("E2E_SKIP_BUILD set — skipping cargo tauri build");
      return;
    }
    const buildArgs = process.env.E2E_RELEASE
      ? ["tauri", "build", "--no-bundle"]
      : ["tauri", "build", "--debug", "--no-bundle"];
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
};
