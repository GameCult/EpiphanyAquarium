import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { createServer } from "vite";

const root = resolve(import.meta.dirname, "..");
const artifactDir = resolve(root, ".epiphany-aquarium");
const desktopScreenshotPath = resolve(artifactDir, "aquarium-focus-smoke-desktop.png");
const wideScreenshotPath = resolve(artifactDir, "aquarium-focus-smoke-wide.png");
const mobileScreenshotPath = resolve(artifactDir, "aquarium-focus-smoke-mobile.png");
const smokePort = Number.parseInt(process.env.EPIPHANY_SMOKE_PORT ?? "1420", 10);
if (!Number.isFinite(smokePort) || smokePort < 1 || smokePort > 65535) {
  throw new Error(`invalid EPIPHANY_SMOKE_PORT: ${process.env.EPIPHANY_SMOKE_PORT}`);
}
const url = `http://127.0.0.1:${smokePort}?smoke=visual`;

await mkdir(artifactDir, { recursive: true });

const server = await createServer({
  root,
  logLevel: "silent",
  server: {
    host: "127.0.0.1",
    port: smokePort,
    strictPort: true,
  },
});

try {
  await server.listen();
  await waitForServer(url);
  const browser = await launchSmokeBrowser();
  const wide = await smokeViewport(browser, { width: 2048, height: 1024 }, wideScreenshotPath, false);
  const desktop = await smokeViewport(browser, { width: 1366, height: 900 }, desktopScreenshotPath, true);
  const mobile = await smokeViewport(browser, { width: 390, height: 844 }, mobileScreenshotPath, false);
  await browser.close();
  console.log(
    JSON.stringify(
      {
        ok: true,
        screenshots: [wideScreenshotPath, desktopScreenshotPath, mobileScreenshotPath],
        probes: { wide, desktop, mobile },
      },
      null,
      2,
    ),
  );
} finally {
  await server.close();
}

async function launchSmokeBrowser() {
  const requestedChannel = process.env.EPIPHANY_SMOKE_BROWSER;
  const channels = requestedChannel ? [requestedChannel] : ["chrome", "msedge"];
  let lastError = null;
  for (const channel of channels) {
    try {
      return await chromium.launch({ channel, headless: true });
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

async function smokeViewport(browser, viewport, screenshotPath) {
  const page = await browser.newPage({ viewport });
  await page.goto(url, { waitUntil: "networkidle" });
  await page.locator(".immersiveShell").waitFor();
  await page.locator('[data-agent-node="coordinator"]').waitFor({ state: "attached" });
  await page.locator(".agentThreeCanvas").waitFor();
  await page.locator(".agentSmokeCanvas").waitFor();
  await page.locator(".agentStardustCanvas").waitFor();
  await page.locator(".agentCrispCanvas").waitFor();
  await page.waitForTimeout(1000);

  const smokeProbe = await probeCanvas(page, ".agentSmokeCanvas");
  const threeProbe = await probeCanvas(page, ".agentThreeCanvas");
  const crispProbe = await probeCanvas(page, ".agentCrispCanvas");
  const stardustProbe = await page.evaluate(() => {
    const canvas = document.querySelector(".agentStardustCanvas");
    return {
      hasCanvas: canvas instanceof HTMLCanvasElement,
      hasWebGpu: Boolean(navigator.gpu),
    };
  });
  if (!smokeProbe.nonBlank) {
    throw new Error(`agent smoke canvas did not render: ${smokeProbe.reason}`);
  }
  if (!threeProbe.nonBlank) {
    throw new Error(`agent three scene did not render: ${threeProbe.reason}`);
  }
  if (!crispProbe.nonBlank) {
    throw new Error(`agent crisp canvas did not render orbit guides: ${crispProbe.reason}`);
  }

  const quietSurface = await page.evaluate(() => !document.querySelector(".agentFocusSurface"));
  if (!quietSurface) {
    throw new Error("operator surface rendered before an aquarium object was selected");
  }

  const projectionProbe = await page.evaluate(() => {
    const node = document.querySelector('[data-agent-node="coordinator"]');
    if (!(node instanceof HTMLElement)) return { ok: false, reason: "coordinator DOM node missing" };
    const style = node.style;
    const x = style.getPropertyValue("--agent-x");
    const y = style.getPropertyValue("--agent-y");
    const z = Number.parseFloat(style.getPropertyValue("--agent-z"));
    const elevation = style.getPropertyValue("--agent-elevation");
    const glow = Number.parseFloat(style.getPropertyValue("--agent-glow-pulse"));
    return {
      ok: x.endsWith("%") && y.endsWith("%") && Number.isFinite(z) && elevation.endsWith("px") && Number.isFinite(glow),
      reason: `x=${x} y=${y} z=${z} elevation=${elevation} glow=${glow}`,
    };
  });
  if (!projectionProbe.ok) {
    throw new Error(`DOM agent projection was not synchronized: ${projectionProbe.reason}`);
  }

  const audioProbe = await page.evaluate(() => window.__epiphanyAquariumAudio ?? null);
  const persistedParams = null;

  await page.screenshot({ path: screenshotPath, fullPage: true });
  const result = await page.evaluate(() => ({
    horizontalOverflow: document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  }));
  await page.close();
  if (result.horizontalOverflow) {
    throw new Error(`visual smoke found horizontal overflow at ${viewport.width}x${viewport.height}`);
  }
  return { smokeProbe, threeProbe, crispProbe, stardustProbe, audioProbe, persistedParams };
}

async function probeCanvas(page, selector) {
  return page.locator(selector).evaluate((canvas) => {
    if (!(canvas instanceof HTMLCanvasElement) || canvas.width === 0 || canvas.height === 0) {
      return { nonBlank: false, reason: "canvas has no drawable dimensions" };
    }
    const gl = canvas.getContext("webgl2");
    if (gl) {
      const width = Math.min(96, canvas.width);
      const height = Math.min(96, canvas.height);
      const points = [
        [0.5, 0.5],
        [0.28, 0.36],
        [0.72, 0.36],
        [0.32, 0.68],
        [0.68, 0.68],
      ];
      let nonBlank = false;
      for (const [xRatio, yRatio] of points) {
        const pixels = new Uint8Array(width * height * 4);
        const x = Math.max(0, Math.floor(canvas.width * xRatio - width * 0.5));
        const y = Math.max(0, Math.floor(canvas.height * yRatio - height * 0.5));
        gl.readPixels(x, y, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        nonBlank ||= pixels.some((value) => value !== 0);
      }
      return {
        nonBlank,
        reason: "webgl2 multi-region sample",
      };
    }
    const context = canvas.getContext("2d");
    if (!context) return { nonBlank: false, reason: "no readable canvas context" };
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    const stride = Math.max(4, Math.floor(pixels.length / 4096 / 4) * 4);
    let nonBlank = false;
    for (let index = 0; index < pixels.length - 3; index += stride) {
      if (pixels[index] !== 0 || pixels[index + 1] !== 0 || pixels[index + 2] !== 0 || pixels[index + 3] !== 0) {
        nonBlank = true;
        break;
      }
    }
    return {
      nonBlank,
      reason: "2d whole-canvas sample",
    };
  });
}

async function waitForServer(target) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(target);
      if (response.ok) return;
    } catch {
      // Server is still waking up.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 500));
  }
  throw new Error("Vite server did not start.");
}
