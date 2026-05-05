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
const url = `http://127.0.0.1:${smokePort}`;
const fluidStorageKey = "epiphany:aquarium-fluid-params:v3";

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

async function smokeViewport(browser, viewport, screenshotPath, exerciseFluidPanel) {
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

  if (viewport.width >= 720) {
    const hoverBox = await page.locator('[data-agent-node="research"] .agentGlyph').boundingBox();
    if (!hoverBox) {
      throw new Error("research DOM node has no hover bounds");
    }
    await page.mouse.move(hoverBox.x + hoverBox.width / 2, hoverBox.y + hoverBox.height / 2);
    await page.waitForTimeout(600);
    let hoverProbe = await page.evaluate(() => {
      const node = document.querySelector('[data-agent-node="research"]');
      if (!(node instanceof HTMLElement)) return { ok: false, reason: "research DOM node missing" };
      const hover = Number.parseFloat(node.style.getPropertyValue("--agent-hover"));
      const acknowledgement = Number.parseFloat(node.style.getPropertyValue("--agent-ack"));
      return {
        ok: hover > 0.7 && acknowledgement >= 0,
        reason: `hover=${hover} ack=${acknowledgement}`,
      };
    });
    if (!hoverProbe.ok) {
      await page.locator('[data-agent-node="research"]').dispatchEvent("pointerenter", { bubbles: true });
      await page.locator('[data-agent-node="research"]').dispatchEvent("mouseenter", { bubbles: true });
      await page.waitForTimeout(600);
      hoverProbe = await page.evaluate(() => {
        const node = document.querySelector('[data-agent-node="research"]');
        if (!(node instanceof HTMLElement)) return { ok: false, reason: "research DOM node missing" };
        const hover = Number.parseFloat(node.style.getPropertyValue("--agent-hover"));
        const acknowledgement = Number.parseFloat(node.style.getPropertyValue("--agent-ack"));
        return {
          ok: hover > 0.7 && acknowledgement >= 0,
          reason: `hover=${hover} ack=${acknowledgement}`,
        };
      });
    }
    if (!hoverProbe.ok) {
      throw new Error(`DOM hover did not reach aquarium projection at ${viewport.width}x${viewport.height}: ${hoverProbe.reason}`);
    }
    const optionProbe = await page.evaluate(() => {
      const halo = document.querySelector('[data-agent-options="research"]');
      if (!(halo instanceof HTMLElement)) return { ok: false, reason: "research options missing" };
      const style = window.getComputedStyle(halo);
      const buttons = [...halo.querySelectorAll("button")].filter((button) => button instanceof HTMLButtonElement);
      const labels = buttons.map((button) => button.textContent?.trim()).filter(Boolean);
      const firstRect = buttons[0]?.getBoundingClientRect();
      const firstStyle = buttons[0] ? window.getComputedStyle(buttons[0]) : null;
      return {
        ok:
          style.opacity !== "0" &&
          firstStyle?.pointerEvents !== "none" &&
          labels.includes("State") &&
          labels.includes("Artifacts") &&
          Boolean(firstRect && firstRect.width > 20 && firstRect.height > 16),
        reason: `opacity=${style.opacity} haloPointer=${style.pointerEvents} buttonPointer=${firstStyle?.pointerEvents} labels=${labels.join(",")}`,
      };
    });
    if (!optionProbe.ok) {
      throw new Error(`DOM agent options did not open for research: ${optionProbe.reason}`);
    }
  }
  let audioProbe = null;
  if (viewport.width >= 720) {
    const researchBox = await page.locator('[data-agent-node="research"] .agentGlyph').boundingBox();
    if (!researchBox) {
      throw new Error("research DOM node has no clickable bounds");
    }
    await page.mouse.click(researchBox.x + researchBox.width / 2, researchBox.y + researchBox.height / 2);
    const focusSurface = await page.evaluate(() => {
      const focus = document.querySelector('.agentStage [data-agent-focus="research"]');
      if (!(focus instanceof HTMLElement)) return { ok: false, reason: "research focus surface missing" };
      const style = window.getComputedStyle(focus);
      const surfaces = [".creatureTreeRoot"].every((selector) => {
        const element = focus.querySelector(selector);
        if (!(element instanceof HTMLElement)) return false;
        const rect = element.getBoundingClientRect();
        return rect.width > 20 && rect.height > 20;
      });
      return {
        ok: style.opacity !== "0" && style.visibility !== "hidden" && surfaces,
        reason: `opacity=${style.opacity} visibility=${style.visibility} surfaces=${surfaces}`,
      };
    });
    if (!focusSurface.ok) {
      throw new Error(`agent-owned focus surface did not open: ${focusSurface.reason}`);
    }
    const organProbe = await page.evaluate(() => {
      const focus = document.querySelector('.agentStage [data-agent-focus="research"]');
      if (!(focus instanceof HTMLElement)) return { ok: false, reason: "research focus surface missing" };
      const root = focus.querySelector(".creatureRootIcon");
      const label = focus.querySelector(".creatureRootSignal span:last-child")?.textContent?.trim();
      const corpseText = focus.textContent ?? "";
      return {
        ok:
          root instanceof HTMLButtonElement &&
          label === "Evidence and artifacts" &&
          !corpseText.includes("Operator Console"),
        reason: `root=${root instanceof HTMLButtonElement} label=${label}`,
      };
    });
    if (!organProbe.ok) {
      throw new Error(`research habitat did not replace the operator console: ${organProbe.reason}`);
    }
    await page.locator('[data-agent-focus="research"] .creatureRootIcon').dispatchEvent("click", { bubbles: true });
    await clickCreatureTreeNode(page, "research", "Heartbeat");
    await clickCreatureTreeNode(page, "research", "Status");
    const heartbeatLeafProbe = await page.evaluate(() => {
      const focus = document.querySelector('.agentStage [data-agent-focus="research"]');
      if (!(focus instanceof HTMLElement)) return { ok: false, reason: "research focus surface missing" };
      const heading = focus.querySelector(".creatureLeafHeader h2")?.textContent?.trim();
      const path = focus.querySelector(".creatureLeafHeader span")?.textContent?.trim();
      const creature = [...focus.querySelectorAll(".facts dt")]
        .find((node) => node.textContent?.trim() === "Creature")
        ?.nextElementSibling?.textContent?.trim();
      return {
        ok: heading === "Status" && path === "Heartbeat / Status" && creature === "Eyes",
        reason: `heading=${heading} path=${path} creature=${creature}`,
      };
    });
    if (!heartbeatLeafProbe.ok) {
      throw new Error(`research heartbeat branch did not unfold as shared creature anatomy: ${heartbeatLeafProbe.reason}`);
    }
    await page.locator('[data-agent-focus="research"] .creatureRootIcon').dispatchEvent("click", { bubbles: true });
    await page.locator('[data-agent-focus="research"] .creatureRootIcon').dispatchEvent("click", { bubbles: true });
    await clickCreatureTreeNode(page, "research", "Evidence");
    await clickCreatureTreeNode(page, "research", "Graph Query");
    const leafProbe = await page.evaluate(() => {
      const focus = document.querySelector('.agentStage [data-agent-focus="research"]');
      if (!(focus instanceof HTMLElement)) return { ok: false, reason: "research focus surface missing" };
      const leaf = focus.querySelector(".creatureLeafSurface");
      const heading = focus.querySelector(".creatureLeafHeader h2")?.textContent?.trim();
      const path = focus.querySelector(".creatureLeafHeader span")?.textContent?.trim();
      return {
        ok: leaf instanceof HTMLElement && heading === "Graph Query" && path === "Evidence / Graph Query",
        reason: `leaf=${leaf instanceof HTMLElement} heading=${heading} path=${path}`,
      };
    });
    if (!leafProbe.ok) {
      throw new Error(`research interaction tree did not unfold to a graph query leaf: ${leafProbe.reason}`);
    }
    try {
      await page.waitForFunction(() => {
        const audio = window.__epiphanyAquariumAudio;
        return audio?.state === "running" &&
          audio.vocalAgentCount >= 8 &&
          audio.lastBurstChirpDrivers >= 6 &&
          audio.spectral?.chirpDrivers === 6 &&
          audio.spectral?.lastBurstChoirVoices >= 3 &&
          audio.spectral?.reactiveFlushes >= 1 &&
          audio.spectral?.transientBins >= 24 &&
          audio.spectral?.vocalAgents >= 8 &&
          audio.spectral?.queuedFrames >= 2048 &&
          audio.lastBurst;
      }, null, { timeout: 5000 });
    } catch (error) {
      const audio = await page.evaluate(() => window.__epiphanyAquariumAudio ?? null);
      throw new Error(`aquarium audio did not wake correctly: ${JSON.stringify(audio)}`, { cause: error });
    }
    audioProbe = await page.evaluate(() => window.__epiphanyAquariumAudio ?? null);
    await page.locator('[data-agent-focus="research"] .creatureLeafHeader button').dispatchEvent("pointerdown", { bubbles: true });
    await page.waitForFunction(() => {
      const audio = window.__epiphanyAquariumAudio;
      return audio?.state === "running" && audio.interfaceHitCount >= 1;
    }, null, { timeout: 5000 });
    audioProbe = await page.evaluate(() => window.__epiphanyAquariumAudio ?? null);
  }

  let persistedParams = null;
  if (exerciseFluidPanel) {
    await page.evaluate((key) => window.localStorage.removeItem(key), fluidStorageKey);
    await page.reload({ waitUntil: "networkidle" });
    await page.locator(".agentSmokeCanvas").waitFor();
    await page.waitForTimeout(700);
    const inspectorGuard = viewport.width >= 720 ? Math.min(230, viewport.height * 0.25) : 0;
    await page.mouse.click(viewport.width - 48, viewport.height - 48 - inspectorGuard);
    await page.waitForTimeout(120);
    const railY = Math.round(viewport.height * 0.47);
    await page.mouse.move(viewport.width - 226, railY);
    await page.mouse.down();
    await page.mouse.move(viewport.width - 46, railY, { steps: 8 });
    await page.mouse.up();
    persistedParams = await page.evaluate((key) => window.localStorage.getItem(key), fluidStorageKey);
    if (!persistedParams || !persistedParams.includes("timeScale")) {
      throw new Error("fluid parameter panel did not persist changed parameters");
    }
  }

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

async function clickCreatureTreeNode(page, agentId, title) {
  const result = await page.evaluate(({ agentId, title }) => {
    const focus = document.querySelector(`[data-agent-focus="${agentId}"]`);
    if (!(focus instanceof HTMLElement)) return { ok: false, reason: "focus missing" };
    const node = [...focus.querySelectorAll(".creatureTreeNode")]
      .find((candidate) => candidate.getAttribute("title") === title);
    if (!(node instanceof HTMLElement)) {
      return {
        ok: false,
        reason: [...focus.querySelectorAll(".creatureTreeNode")]
          .map((candidate) => candidate.getAttribute("title") ?? candidate.textContent?.trim())
          .join(", "),
      };
    }
    node.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    return { ok: true, reason: title };
  }, { agentId, title });
  if (!result.ok) {
    throw new Error(`creature tree node ${agentId}/${title} was not clickable: ${result.reason}`);
  }
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
