"use strict";

/**
 * Ghost Music desktop shell.
 *
 * The Next.js production server is bundled in resources/app and started as a
 * child Node process (Electron itself, with ELECTRON_RUN_AS_NODE) on a free
 * loopback port. The window then loads that local origin, which keeps service
 * workers, range requests, downloads, and the Web Audio analyser working
 * exactly as they do on the web.
 */

const { app, BrowserWindow, Menu, dialog, shell } = require("electron");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const http = require("node:http");
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");

const isPackaged = app.isPackaged;
const appRoot = isPackaged ? path.join(process.resourcesPath, "app") : path.join(__dirname, "..");
const externalUrl = process.env.GHOST_SERVER_URL || "";

let serverProcess = null;
let serverPort = 0;
let lanMode = false;
let mainWindow = null;
let splashWindow = null;
let quitting = false;
let logStream = null;

app.commandLine.appendSwitch("autoplay-policy", "no-user-gesture-required");
app.setAppUserModelId("com.ghostmusic.desktop");

const singleInstance = app.requestSingleInstanceLock();
if (!singleInstance) {
  app.quit();
} else {
  app.on("second-instance", () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });
}

/* --------------------------------------------------------------- helpers --- */
function freePort(preferred = 0) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", () => {
      if (!preferred) return reject(new Error("No free port available"));
      freePort(0).then(resolve, reject);
    });
    server.listen(preferred, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

// A stable port in phone mode means the address stays the same across launches.
const LAN_PORT = 43110;

function log(message) {
  const line = `[${new Date().toISOString()}] ${message}\n`;
  if (logStream) logStream.write(line);
  if (!isPackaged) process.stdout.write(line);
}

function logPath() {
  return path.join(app.getPath("userData"), "logs", "server.log");
}

function openLogStream() {
  fs.mkdirSync(path.dirname(logPath()), { recursive: true });
  logStream = fs.createWriteStream(logPath(), { flags: "a" });
}

function lanAddress(port) {
  for (const interfaces of Object.values(os.networkInterfaces())) {
    for (const entry of interfaces || []) {
      if (entry.family === "IPv4" && !entry.internal) return `http://${entry.address}:${port}`;
    }
  }
  return "";
}

function health(port) {
  return new Promise(resolve => {
    const request = http.get({ host: "127.0.0.1", port, path: "/api/health", timeout: 2000 }, response => {
      let body = "";
      response.on("data", chunk => (body += chunk));
      response.on("end", () => {
        try {
          resolve(response.statusCode === 200 && JSON.parse(body).ok === true);
        } catch {
          resolve(false);
        }
      });
    });
    request.on("error", () => resolve(false));
    request.on("timeout", () => {
      request.destroy();
      resolve(false);
    });
  });
}

const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

async function waitForServer(port, child) {
  const deadline = Date.now() + 120000;
  while (Date.now() < deadline) {
    if (await health(port)) return true;
    if (child && child.exitCode !== null) return false;
    await wait(350);
  }
  return false;
}

/* ---------------------------------------------------------------- server --- */
async function startServer({ lan = false } = {}) {
  if (externalUrl) return externalUrl;
  if (serverProcess) await stopServer();
  lanMode = lan;
  serverPort = await freePort(lan ? LAN_PORT : 0);
  const dataDir = path.join(app.getPath("userData"), "data");
  fs.mkdirSync(dataDir, { recursive: true });

  const entry = path.join(appRoot, "server.js");
  if (!fs.existsSync(entry)) throw new Error(`The bundled server is missing at ${entry}`);

  log(`starting server on port ${serverPort} (lan=${lan}) from ${appRoot}`);
  serverProcess = spawn(process.execPath, [entry], {
    cwd: appRoot,
    windowsHide: true,
    env: {
      ...process.env,
      ELECTRON_RUN_AS_NODE: "1",
      NODE_ENV: "production",
      PORT: String(serverPort),
      HOSTNAME: lan ? "0.0.0.0" : "127.0.0.1",
      GHOST_DATA_DIR: dataDir,
      GHOST_DESKTOP: "1",
      NEXT_TELEMETRY_DISABLED: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });

  serverProcess.stdout.on("data", chunk => log(`out: ${chunk.toString().trimEnd()}`));
  serverProcess.stderr.on("data", chunk => log(`err: ${chunk.toString().trimEnd()}`));
  serverProcess.on("exit", code => {
    log(`server exited with code ${code}`);
    if (!quitting && mainWindow) {
      dialog.showMessageBox(mainWindow, {
        type: "error",
        title: "Ghost Music stopped",
        message: "The local music server stopped unexpectedly.",
        detail: `Exit code ${code}. Logs: ${logPath()}`,
        buttons: ["Restart", "Quit"],
        defaultId: 0,
      }).then(({ response }) => (response === 0 ? restart() : app.quit()));
    }
  });

  if (!(await waitForServer(serverPort, serverProcess))) {
    throw new Error(`The local server did not become ready. See ${logPath()}`);
  }
  return `http://127.0.0.1:${serverPort}`;
}

async function stopServer() {
  const child = serverProcess;
  serverProcess = null;
  if (!child) return;
  child.removeAllListeners("exit");
  await new Promise(resolve => {
    child.once("exit", resolve);
    child.kill();
    setTimeout(resolve, 4000);
  });
}

async function restart() {
  try {
    const url = await startServer({ lan: lanMode });
    if (mainWindow) await mainWindow.loadURL(url);
  } catch (error) {
    dialog.showErrorBox("Ghost Music could not restart", String(error.message || error));
  }
}

/* ---------------------------------------------------------------- windows -- */
function createSplash() {
  splashWindow = new BrowserWindow({
    width: 460,
    height: 300,
    frame: false,
    resizable: false,
    transparent: false,
    show: true,
    backgroundColor: "#0b0b0e",
    webPreferences: { contextIsolation: true, nodeIntegration: false },
  });
  splashWindow.loadFile(path.join(__dirname, "splash.html"));
}

function createWindow(url) {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1024,
    minHeight: 680,
    show: false,
    backgroundColor: "#101012",
    autoHideMenuBar: true,
    title: "Ghost Music",
    icon: path.join(__dirname, "icon.png"),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      spellcheck: false,
      backgroundThrottling: false,
    },
  });

  mainWindow.once("ready-to-show", () => {
    if (splashWindow && !splashWindow.isDestroyed()) {
      splashWindow.destroy();
      splashWindow = null;
    }
    mainWindow.show();
  });

  mainWindow.webContents.setWindowOpenHandler(({ url: target }) => {
    if (/^https?:/i.test(target) && !target.startsWith(url)) void shell.openExternal(target);
    return { action: "deny" };
  });

  mainWindow.webContents.on("will-navigate", (event, target) => {
    if (!target.startsWith(url)) {
      event.preventDefault();
      if (/^https?:/i.test(target)) void shell.openExternal(target);
    }
  });

  mainWindow.on("closed", () => {
    mainWindow = null;
  });

  return mainWindow.loadURL(url);
}

function buildMenu() {
  const template = [
    {
      label: "Ghost Music",
      submenu: [
        {
          label: lanMode ? "Phone connection is ON" : "Connect a phone on this network",
          enabled: !lanMode && !externalUrl,
          click: async () => {
            const answer = await dialog.showMessageBox(mainWindow, {
              type: "question",
              title: "Connect a phone",
              message: "Share this library with the Ghost Music app on your phone?",
              detail: "Ghost Music will restart listening on your local network so the Android app can connect. Anything else on the same Wi-Fi can reach it too, so use this on a network you trust.",
              buttons: ["Cancel", "Enable phone connection"],
              defaultId: 1,
              cancelId: 0,
            });
            if (answer.response !== 1) return;
            try {
              const url = await startServer({ lan: true });
              const address = lanAddress(serverPort) || url;
              await dialog.showMessageBox(mainWindow, {
                type: "info",
                title: "Ready for your phone",
                message: "Enter this address in the Ghost Music Android app:",
                detail: address,
                buttons: ["Copy", "Close"],
                defaultId: 0,
              });
              const { clipboard } = require("electron");
              clipboard.writeText(address);
              buildMenu();
              if (mainWindow) await mainWindow.loadURL(url);
            } catch (error) {
              dialog.showErrorBox("Could not start the shared server", String(error.message || error));
            }
          },
        },
        externalUrl ? null : {
          label: "Show server address",
          click: () => dialog.showMessageBox(mainWindow, {
            type: "info",
            title: "Server address",
            message: lanMode ? (lanAddress(serverPort) || `http://127.0.0.1:${serverPort}`) : `http://127.0.0.1:${serverPort}`,
            buttons: ["OK"],
          }),
        },
        { type: "separator" },
        { label: "Open data folder", click: () => void shell.openPath(app.getPath("userData")) },
        { label: "Open server log", click: () => void shell.openPath(logPath()) },
        { type: "separator" },
        { role: "quit" },
      ].filter(Boolean),
    },
    {
      label: "Edit",
      submenu: [{ role: "undo" }, { role: "redo" }, { type: "separator" }, { role: "cut" }, { role: "copy" }, { role: "paste" }, { role: "selectAll" }],
    },
    {
      label: "View",
      submenu: [
        { role: "reload" },
        { role: "forceReload" },
        { type: "separator" },
        { role: "resetZoom" },
        { role: "zoomIn" },
        { role: "zoomOut" },
        { type: "separator" },
        { role: "togglefullscreen" },
        { role: "toggleDevTools" },
      ],
    },
  ];
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

/* ------------------------------------------------------------------- boot --- */
app.whenReady().then(async () => {
  openLogStream();
  buildMenu();
  createSplash();
  try {
    const url = await startServer({ lan: false });
    await createWindow(url);
  } catch (error) {
    log(`fatal: ${error && error.stack ? error.stack : error}`);
    if (splashWindow && !splashWindow.isDestroyed()) splashWindow.destroy();
    dialog.showErrorBox(
      "Ghost Music could not start",
      `${error.message || error}\n\nLog file: ${logPath()}`,
    );
    app.quit();
  }

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0 && serverPort) {
      void createWindow(`http://127.0.0.1:${serverPort}`);
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("before-quit", event => {
  if (quitting) return;
  quitting = true;
  if (serverProcess) {
    event.preventDefault();
    const child = serverProcess;
    serverProcess = null;
    child.removeAllListeners("exit");
    child.once("exit", () => app.quit());
    child.kill();
    setTimeout(() => app.quit(), 3000);
  }
});
