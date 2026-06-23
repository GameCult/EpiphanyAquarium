#!/usr/bin/env node
import Module, { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

process.env.NODE_PATH = [
  path.resolve(repoRoot, "..", "CultLib", "packages"),
  process.env.NODE_PATH || "",
].filter(Boolean).join(path.delimiter);
Module._initPaths();

const { defineDocumentType } = require("cultcache-ts");
const { CultMesh } = require("cultmesh-ts");
const { defineCultNetDocumentBinding } = require("cultnet-ts");

const providerId = "epiphany.aquarium";
const providerAdvertisementDefinition = defineDocumentType({
  type: "gamecult.eve.provider_advertisement",
  schemaName: "gamecult.eve.provider_advertisement",
  schemaId: "gamecult.eve.provider_advertisement.v1",
  schemaVersion: "gamecult.eve.provider_advertisement.v1",
  global: true,
  name: (value) => value?.providerId || providerId,
  schema: { parse: (value) => value },
  members: [
    { slot: 0, memberName: "providerId", typeName: "string", isName: true },
    { slot: 1, memberName: "serviceId", typeName: "string" },
    { slot: 2, memberName: "verseId", typeName: "string" },
    { slot: 3, memberName: "title", typeName: "string" },
    { slot: 4, memberName: "description", typeName: "string" },
    { slot: 5, memberName: "canonicalService", typeName: "string" },
    { slot: 6, memberName: "locatedService", typeName: "string" },
    { slot: 7, memberName: "cultMeshAddress", typeName: "string" },
    { slot: 8, memberName: "status", typeName: "string" },
    { slot: 9, memberName: "updatedAt", typeName: "string" },
    { slot: 10, memberName: "capabilities", typeName: "array" },
    { slot: 11, memberName: "endpoints", typeName: "array" },
    { slot: 12, memberName: "routes", typeName: "array" },
  ],
});

main().catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exitCode = 1;
});

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const endpoint = args["odin-cultmesh-rudp"] || process.env.EPIPHANY_AQUARIUM_ODIN_CULTMESH_RUDP || "";
  if (!endpoint) {
    throw new Error("EpiphanyAquarium Odin publication requires --odin-cultmesh-rudp <host:port> or EPIPHANY_AQUARIUM_ODIN_CULTMESH_RUDP.");
  }

  const advertisement = buildAdvertisement();
  await CultMesh.publishRudpDocumentOnce(
    "epiphany-aquarium",
    0x0d1d0002,
    normalizeRudpEndpoint(endpoint),
    defineCultNetDocumentBinding({ definition: providerAdvertisementDefinition }),
    advertisement.providerId,
    advertisement,
    {
      sourceRole: "epiphany-aquarium.provider",
      tags: ["startup-respect", "odin-verse-discovery"],
    },
  );

  console.log(JSON.stringify({
    ok: true,
    schemaId: "gamecult.eve.provider_advertisement.v1",
    providerId: advertisement.providerId,
    odinCultMeshRudp: endpoint,
  }, null, 2));
}

function buildAdvertisement() {
  const observedAt = new Date().toISOString();
  return {
    providerId,
    serviceId: "epiphany-aquarium.interface-organism",
    verseId: "starfire.local",
    title: "Epiphany Aquarium",
    description: "Fullscreen React/Tauri/WebGL interface organism for Epiphany's operator-visible agent, memory, control, and evidence surfaces.",
    canonicalService: "asgard.epiphany.aquarium",
    locatedService: "asgard.starfire.epiphany-aquarium",
    cultMeshAddress: "asgard.starfire.epiphany-aquarium/interface",
    status: "interface-provider-advertisement",
    updatedAt: observedAt,
    capabilities: [
      "epiphany-operator-interface",
      "agent-state-projection",
      "evidence-surface",
      "tauri-shell",
      "webgl-aquarium",
    ],
    endpoints: [
      { transport: "vite-dev", address: "http://127.0.0.1:1420/" },
      { transport: "tauri", address: "src-tauri" },
      { transport: "repo-state", address: "state/map.yaml" },
    ],
    routes: [
      { transport: "repo-cli", address: "npm run build", role: "build" },
      { transport: "repo-cli", address: "npm run smoke:visual", role: "visual-smoke" },
      { transport: "repo-cli", address: "npm run provider:publish-odin", role: "provider-advertisement" },
    ],
    commandSurface: {
      mode: "renderer-input",
      note: "Aquarium surfaces inspect and request through the Epiphany backend boundary; they do not own Epiphany durable truth.",
    },
    authority: {
      owner: "EpiphanyAquarium",
      owns: [
        "visual interaction grammar",
        "Tauri/React/WebGL interface surface",
        "operator-visible projection affordances",
      ],
      doesNotOwn: [
        "Epiphany harness backend truth",
        "agent private state",
        "Odin discovery truth",
      ],
    },
  };
}

function normalizeRudpEndpoint(value) {
  const text = String(value || "").trim();
  if (!text) throw new Error("Odin CultMesh/RUDP endpoint must be non-empty.");
  if (text.startsWith("rudp://")) return text;
  const ipv6 = text.match(/^\[([^\]]+)\]:(\d+)$/);
  if (ipv6) return `rudp://[${ipv6[1]}]:${ipv6[2]}`;
  const index = text.lastIndexOf(":");
  if (index <= 0 || index === text.length - 1) {
    throw new Error(`Odin CultMesh/RUDP endpoint must be host:port, got "${value}".`);
  }
  return `rudp://${text.slice(0, index)}:${text.slice(index + 1)}`;
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg.startsWith("--")) continue;
    const key = arg.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith("--")) {
      parsed[key] = true;
    } else {
      parsed[key] = next;
      index += 1;
    }
  }
  return parsed;
}
