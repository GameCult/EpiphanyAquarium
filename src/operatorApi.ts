import { invoke } from "@tauri-apps/api/core";
import type { OperatorAction, OperatorActionResult, OperatorSnapshot, StatusRequest, SwarmMember } from "./types";

export async function loadOperatorSnapshot(request: StatusRequest): Promise<OperatorSnapshot> {
  if (hasTauriRuntime()) {
    return invoke<OperatorSnapshot>("load_operator_snapshot", { request });
  }

  const response = await fetch("/operator-snapshot.sample.json", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`sample operator snapshot failed: ${response.status}`);
  }
  const sample = (await response.json()) as OperatorSnapshot;
  const swarmMembers = sample.swarmMembers ?? sampleSwarmMembers;
  return {
    ...sample,
    activeMember: swarmMembers.find((member) => member.id === request.memberId) ?? swarmMembers[0],
    swarmMembers,
    communications: sample.communications ?? sampleCommunications,
  };
}

export async function runOperatorAction(action: OperatorAction, request: StatusRequest): Promise<OperatorActionResult> {
  if (hasTauriRuntime()) {
    return invoke<OperatorActionResult>("run_operator_action", { action, request });
  }

  await new Promise((resolve) => setTimeout(resolve, 250));
  return {
    action,
    artifactPath: "E:\\Projects\\EpiphanyAquarium\\.epiphany-aquarium\\sample-action",
    summary: action === "requestSwarmHelp" ? "Asked Aetheria Lore coordinator for help." : `${action} sample completed.`,
    threadId: action === "prepareCheckpoint" ? "019dd9d1-045b-7f13-b0e1-38ed89b31495" : request.threadId,
  };
}

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const sampleSwarmMembers: SwarmMember[] = [
  {
    id: "epiphany-agent",
    label: "Epiphany",
    kind: "harness",
    harnessRoot: "E:\\Projects\\EpiphanyAgent",
    workspaceRoot: "E:\\Projects\\EpiphanyAgent",
    stateRoot: "E:\\Projects\\EpiphanyAgent\\state",
    codexHome: "E:\\Projects\\EpiphanyAgent\\.epiphany-gui\\codex-home",
    artifactRoot: "E:\\Projects\\EpiphanyAgent\\.epiphany-gui",
    description: "Main harness instance",
    status: "active",
  },
  {
    id: "aetheria-lore",
    label: "Aetheria Lore",
    kind: "workspace",
    harnessRoot: "E:\\Projects\\EpiphanyAgent",
    workspaceRoot: "E:\\Projects\\AetheriaLore",
    stateRoot: "E:\\Projects\\AetheriaLore\\.epiphany",
    codexHome: "E:\\Projects\\AetheriaLore\\.epiphany\\codex-home",
    artifactRoot: "E:\\Projects\\AetheriaLore\\.epiphany\\artifacts",
    description: "Vault and website swarm instance",
    status: "bootstrap",
  },
];

const sampleCommunications = [
  {
    id: "sample-swarm-communication",
    createdAt: "1777980000000",
    fromMemberId: "epiphany-agent",
    toMemberId: "aetheria-lore",
    kind: "request" as const,
    status: "open" as const,
    subject: "Ask the lore coordinator",
    body: "Epiphany needs Aetheria Lore to inspect its own vault and call back through the coordinator lane.",
    artifactPath: "E:\\Projects\\EpiphanyAquarium\\.epiphany-aquarium\\swarm-communications.jsonl",
  },
];
