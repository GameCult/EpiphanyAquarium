import { invoke } from "@tauri-apps/api/core";
import type { OperatorAction, OperatorActionResult, OperatorSnapshot, StatusRequest } from "./types";

export async function loadOperatorSnapshot(request: StatusRequest): Promise<OperatorSnapshot> {
  if (hasTauriRuntime()) {
    return invoke<OperatorSnapshot>("load_operator_snapshot", { request });
  }

  const response = await fetch("/operator-snapshot.sample.json", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`sample operator snapshot failed: ${response.status}`);
  }
  return (await response.json()) as OperatorSnapshot;
}

export async function runOperatorAction(action: OperatorAction, request: StatusRequest): Promise<OperatorActionResult> {
  if (hasTauriRuntime()) {
    return invoke<OperatorActionResult>("run_operator_action", { action, request });
  }

  await new Promise((resolve) => setTimeout(resolve, 250));
  return {
    action,
    artifactPath: "E:\\Projects\\EpiphanyAquarium\\.epiphany-aquarium\\sample-action",
    summary: `${action} sample completed.`,
    threadId: action === "prepareCheckpoint" ? "019dd9d1-045b-7f13-b0e1-38ed89b31495" : request.threadId,
  };
}

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
