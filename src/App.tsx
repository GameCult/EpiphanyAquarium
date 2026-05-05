import {
  Boxes,
  AlertTriangle,
  BriefcaseBusiness,
  CheckCircle2,
  ClipboardCheck,
  Database,
  Eye,
  FileText,
  GitBranch,
  ListChecks,
  Map,
  Play,
  RefreshCw,
} from "lucide-react";
import { EpiphanyGraphViewer, validateEpiphanyGraphsState } from "@epiphanygraph/epiphany-graph-viewer";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createAquariumRenderer } from "./aquariumFluid";
import { createHarmonyRuntime, loadNextHarmony, loadShuffledDefaultHarmony, pickHarmonyFolder } from "./midiHarmony";
import { loadOperatorSnapshot, runOperatorAction } from "./operatorApi";
import type { ArtifactBundle, OperatorAction, OperatorActionResult, OperatorSnapshot, StatusRequest } from "./types";
import type { EpiphanyCodeRef, EpiphanyGraphsState } from "@epiphanygraph/epiphany-graph-viewer";
import type { AquariumAgentProjection, AquariumOptionFrame, AquariumRenderer, AquariumUiFrame } from "./aquariumFluid";
import type { AquariumHarmonyFrame, HarmonyRuntime, HarmonySource, MidiCorpusFile } from "./midiHarmony";

const roleOrder = ["implementation", "imagination", "research", "eyes", "modeling", "verification", "reorientation"];
const constellationSpecs = [
  {
    id: "coordinator",
    laneId: "coordinator",
    name: "Self",
    title: "Coordinator",
    glyph: "S",
    shape: "core",
    color: "#f15f45",
    glow: "#f7bd58",
    baseX: 49,
    baseY: 43,
    driftX: 1.9,
    driftY: 1.4,
    phase: 0.2,
  },
  {
    id: "imagination",
    laneId: "imagination",
    name: "Imagination",
    title: "Planner",
    glyph: "I",
    shape: "kite",
    color: "#9f6ee7",
    glow: "#eb8fc9",
    baseX: 29,
    baseY: 25,
    driftX: 2.3,
    driftY: 1.6,
    phase: 1.4,
  },
  {
    id: "research",
    laneId: "research",
    name: "Eyes",
    title: "Research",
    glyph: "E",
    shape: "lens",
    color: "#2877b8",
    glow: "#63c5da",
    baseX: 80,
    baseY: 23,
    driftX: 2.1,
    driftY: 1.7,
    phase: 2.1,
  },
  {
    id: "modeling",
    laneId: "modeling",
    name: "Body",
    title: "Modeling",
    glyph: "B",
    shape: "hex",
    color: "#2f9b67",
    glow: "#92d876",
    baseX: 21,
    baseY: 58,
    driftX: 1.8,
    driftY: 1.5,
    phase: 3.2,
  },
  {
    id: "implementation",
    laneId: "implementation",
    name: "Hands",
    title: "Implementation",
    glyph: "H",
    shape: "capsule",
    color: "#cf5a2f",
    glow: "#f1ad4e",
    baseX: 48,
    baseY: 64,
    driftX: 2.6,
    driftY: 1.2,
    phase: 4.4,
  },
  {
    id: "verification",
    laneId: "verification",
    name: "Soul",
    title: "Verification",
    glyph: "V",
    shape: "diamond",
    color: "#4e63b6",
    glow: "#a6a9f4",
    baseX: 78,
    baseY: 61,
    driftX: 1.7,
    driftY: 1.9,
    phase: 5.2,
  },
  {
    id: "reorientation",
    laneId: "reorientation",
    name: "Life",
    title: "Continuity",
    glyph: "L",
    shape: "seed",
    color: "#148d87",
    glow: "#58ddc4",
    baseX: 63,
    baseY: 29,
    driftX: 1.4,
    driftY: 2.2,
    phase: 6.0,
  },
] as const;
const harmonyAgentIds = constellationSpecs.map((agent) => agent.id);

type ConstellationSpec = (typeof constellationSpecs)[number];
type ProjectedAgent = ConstellationSpec & {
  status: string;
  tone: string;
  thought: string;
  detail: string;
  activity: number;
  jobs: number;
  review: string;
};
type AquariumOption = {
  label: string;
  deck?: DeckId;
  subdeck?: string;
  action?: OperatorAction;
};
const deckSubmenus = {
  command: ["run", "connection", "signals"],
  state: ["environment", "planning", "graph"],
  agents: ["lanes", "findings", "jobs"],
  artifacts: ["bundles"],
} as const;
const deckLabels: Record<keyof typeof deckSubmenus, string> = {
  command: "Command",
  state: "State",
  agents: "Agents",
  artifacts: "Artifacts",
};
type DeckId = keyof typeof deckSubmenus;
const aquariumOptionsByAgent: Record<string, AquariumOption[]> = {
  coordinator: [
    { label: "Signals", deck: "command", subdeck: "signals" },
    { label: "Run", deck: "command", subdeck: "run" },
    { label: "Checkpoint", action: "prepareCheckpoint" },
  ],
  imagination: [
    { label: "Planning", deck: "state", subdeck: "planning" },
    { label: "Launch", action: "launchImagination" },
    { label: "Read", action: "readImaginationResult" },
    { label: "Accept", action: "acceptImagination" },
  ],
  research: [
    { label: "State", deck: "state", subdeck: "graph" },
    { label: "Artifacts", deck: "artifacts", subdeck: "bundles" },
  ],
  modeling: [
    { label: "Graph", deck: "state", subdeck: "graph" },
    { label: "Launch", action: "launchModeling" },
    { label: "Read", action: "readModelingResult" },
    { label: "Accept", action: "acceptModeling" },
  ],
  implementation: [
    { label: "Run", deck: "command", subdeck: "run" },
    { label: "Continue", action: "continueImplementation" },
    { label: "Artifacts", deck: "artifacts", subdeck: "bundles" },
  ],
  verification: [
    { label: "Findings", deck: "agents", subdeck: "findings" },
    { label: "Launch", action: "launchVerification" },
    { label: "Read", action: "readVerificationResult" },
    { label: "Accept", action: "acceptVerification" },
  ],
  reorientation: [
    { label: "Continuity", deck: "command", subdeck: "signals" },
    { label: "Launch", action: "launchReorient" },
    { label: "Read", action: "readReorientResult" },
    { label: "Accept", action: "acceptReorient" },
  ],
};
const actionButtons: Array<{
  action: OperatorAction;
  label: string;
  runningLabel: string;
  title: string;
  requiresThread?: boolean;
  requiresReadyState?: boolean;
  requiresImaginationPatch?: boolean;
  requiresModelingPatch?: boolean;
  requiresVerificationResult?: boolean;
  requiresReorientResult?: boolean;
  requiresPlanningDraft?: boolean;
  requiresContinueImplementation?: boolean;
  icon: "file" | "check" | "play" | "eye" | "accept" | "runtime" | "plan" | "ide";
}> = [
  {
    action: "statusSnapshot",
    label: "Status Snapshot",
    runningLabel: "Writing",
    title: "Write an auditable status snapshot",
    icon: "file",
  },
  {
    action: "coordinatorPlan",
    label: "Coordinator Plan",
    runningLabel: "Running",
    title: "Run a review-gated coordinator plan",
    icon: "check",
  },
  {
    action: "inspectUnity",
    label: "Inspect Unity",
    runningLabel: "Inspecting",
    title: "Resolve the project-pinned Unity editor and write runtime artifacts",
    icon: "runtime",
  },
  {
    action: "inspectRider",
    label: "Inspect Rider",
    runningLabel: "Inspecting",
    title: "Inspect Rider, solution, and source control status through the local bridge",
    icon: "ide",
  },
  {
    action: "prepareCheckpoint",
    label: "Prepare Checkpoint",
    runningLabel: "Preparing",
    title: "Seed durable Epiphany state for this GUI operator thread",
    icon: "accept",
  },
  {
    action: "adoptObjectiveDraft",
    label: "Adopt Draft",
    runningLabel: "Adopting",
    title: "Adopt the selected Objective Draft as the active implementation objective",
    requiresThread: true,
    requiresReadyState: true,
    requiresPlanningDraft: true,
    icon: "plan",
  },
  {
    action: "continueImplementation",
    label: "Continue Implementation",
    runningLabel: "Implementing",
    title: "Run a bounded implementation turn when the coordinator has cleared specialist lanes",
    requiresThread: true,
    requiresReadyState: true,
    requiresContinueImplementation: true,
    icon: "play",
  },
  {
    action: "launchImagination",
    label: "Launch Imagination",
    runningLabel: "Launching",
    title: "Launch the fixed imagination/planning worker for this thread",
    requiresThread: true,
    requiresReadyState: true,
    icon: "play",
  },
  {
    action: "readImaginationResult",
    label: "Read Imagination",
    runningLabel: "Reading",
    title: "Read the latest imagination/planning finding",
    requiresThread: true,
    icon: "eye",
  },
  {
    action: "acceptImagination",
    label: "Accept Imagination",
    runningLabel: "Accepting",
    title: "Accept a reviewed planning-only patch into Epiphany state",
    requiresThread: true,
    requiresReadyState: true,
    requiresImaginationPatch: true,
    icon: "accept",
  },
  {
    action: "launchModeling",
    label: "Launch Modeling",
    runningLabel: "Launching",
    title: "Launch the fixed modeling/checkpoint worker for this thread",
    requiresThread: true,
    requiresReadyState: true,
    icon: "play",
  },
  {
    action: "readModelingResult",
    label: "Read Modeling",
    runningLabel: "Reading",
    title: "Read the latest modeling/checkpoint finding",
    requiresThread: true,
    icon: "eye",
  },
  {
    action: "acceptModeling",
    label: "Accept Modeling",
    runningLabel: "Accepting",
    title: "Accept a reviewed modeling graph/checkpoint patch into Epiphany state",
    requiresThread: true,
    requiresReadyState: true,
    requiresModelingPatch: true,
    icon: "accept",
  },
  {
    action: "launchVerification",
    label: "Launch Verification",
    runningLabel: "Launching",
    title: "Launch the fixed verification/review worker for this thread",
    requiresThread: true,
    requiresReadyState: true,
    icon: "play",
  },
  {
    action: "readVerificationResult",
    label: "Read Verification",
    runningLabel: "Reading",
    title: "Read the latest verification/review finding",
    requiresThread: true,
    icon: "eye",
  },
  {
    action: "acceptVerification",
    label: "Accept Verification",
    runningLabel: "Accepting",
    title: "Accept a reviewed verification finding into Epiphany state",
    requiresThread: true,
    requiresReadyState: true,
    requiresVerificationResult: true,
    icon: "accept",
  },
  {
    action: "launchReorient",
    label: "Launch Reorient",
    runningLabel: "Launching",
    title: "Launch the bounded reorient-worker for this thread",
    requiresThread: true,
    requiresReadyState: true,
    icon: "play",
  },
  {
    action: "readReorientResult",
    label: "Read Reorient",
    runningLabel: "Reading",
    title: "Read the latest reorient-worker finding",
    requiresThread: true,
    icon: "eye",
  },
  {
    action: "acceptReorient",
    label: "Accept Reorient",
    runningLabel: "Accepting",
    title: "Accept a completed reorient-worker finding into Epiphany state",
    requiresThread: true,
    requiresReadyState: true,
    requiresReorientResult: true,
    icon: "accept",
  },
];

function text(value: unknown, fallback = "none"): string {
  if (value === null || value === undefined || value === "") {
    return fallback;
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return fallback;
}

function displayLabel(value: unknown, fallback = "none"): string {
  return text(value, fallback).replace(/([a-z])([A-Z])/g, "$1 $2");
}

function listText(value: unknown, fallback = "none"): string {
  return Array.isArray(value) && value.length > 0 ? value.map(String).join(", ") : fallback;
}

function objectList(value: unknown): any[] {
  return Array.isArray(value) ? value.filter((item) => item && typeof item === "object") : [];
}

function countText(value: unknown): string {
  return typeof value === "number" ? String(value) : text(value, "0");
}

function statusClass(value: unknown): string {
  const lower = text(value).toLowerCase();
  if (lower.includes("blocked") || lower.includes("critical") || lower.includes("regather")) return "danger";
  if (lower.includes("needed") || lower.includes("review") || lower.includes("prepare") || lower.includes("high")) return "warn";
  if (lower.includes("completed") || lower.includes("ready") || lower.includes("continue") || lower.includes("pass")) return "ok";
  return "neutral";
}

function projectedThought(value: unknown, fallback: string): string {
  const cleaned = text(value, fallback).replace(/\s+/g, " ").trim();
  if (cleaned.length <= 136) return cleaned;
  return `${cleaned.slice(0, 132).trim()}...`;
}

function projectedActivity(status: unknown, jobCount = 0): number {
  const lower = text(status).toLowerCase();
  const jobBoost = Math.min(jobCount * 0.06, 0.18);
  if (lower.includes("critical") || lower.includes("panic") || lower.includes("fatal")) return 1;
  if (lower.includes("failed") || lower.includes("error") || lower.includes("high")) return 0.74 + jobBoost;
  if (lower.includes("running") || lower.includes("launch") || lower.includes("active")) return 0.58 + jobBoost;
  if (lower.includes("blocked") || lower.includes("needed") || lower.includes("regather")) return 0.38 + jobBoost;
  if (lower.includes("prepare") || lower.includes("review") || lower.includes("ready")) return 0.3 + jobBoost;
  if (lower.includes("completed") || lower.includes("continue") || lower.includes("pass")) return 0.24 + jobBoost;
  if (lower.includes("idle")) return 0.12 + jobBoost;
  return 0.22 + jobBoost;
}

function findingSummary(result: any): string | undefined {
  const finding = result?.finding;
  return finding?.summary ?? finding?.nextSafeMove ?? finding?.mode ?? finding?.verdict ?? result?.note;
}

const emptyGraphState: EpiphanyGraphsState = {
  architecture: { nodes: [], edges: [] },
  dataflow: { nodes: [], edges: [] },
  links: [],
};

function normalizeGraphState(value: any): EpiphanyGraphsState {
  if (!value || typeof value !== "object") return emptyGraphState;
  return {
    architecture: normalizeGraph(value.architecture),
    dataflow: normalizeGraph(value.dataflow),
    links: objectList(value.links).map((link) => ({
      dataflow_node_id: text(link.dataflow_node_id ?? link.dataflowNodeId, ""),
      architecture_node_id: text(link.architecture_node_id ?? link.architectureNodeId, ""),
      relationship: link.relationship ?? null,
      code_refs: normalizeCodeRefs(link.code_refs ?? link.codeRefs),
    })).filter((link) => link.dataflow_node_id && link.architecture_node_id),
  };
}

function normalizeGraph(value: any) {
  return {
    nodes: objectList(value?.nodes).map((node) => ({
      id: text(node.id, ""),
      title: text(node.title ?? node.id, "Untitled node"),
      purpose: text(node.purpose ?? node.summary, "No purpose recorded."),
      mechanism: node.mechanism ?? null,
      metaphor: node.metaphor ?? null,
      status: node.status ?? null,
      code_refs: normalizeCodeRefs(node.code_refs ?? node.codeRefs),
    })).filter((node) => node.id),
    edges: objectList(value?.edges).map((edge) => ({
      id: edge.id ?? null,
      source_id: text(edge.source_id ?? edge.sourceId, ""),
      target_id: text(edge.target_id ?? edge.targetId, ""),
      kind: text(edge.kind, "link"),
      label: edge.label ?? null,
      mechanism: edge.mechanism ?? null,
      code_refs: normalizeCodeRefs(edge.code_refs ?? edge.codeRefs),
    })).filter((edge) => edge.source_id && edge.target_id),
  };
}

function normalizeCodeRefs(value: any): EpiphanyCodeRef[] {
  return objectList(value).map((ref) => ({
    path: text(ref.path, ""),
    start_line: typeof ref.start_line === "number" ? ref.start_line : ref.startLine,
    end_line: typeof ref.end_line === "number" ? ref.end_line : ref.endLine,
    symbol: ref.symbol ?? null,
    note: ref.note ?? null,
  })).filter((ref) => ref.path);
}

function graphRecordCount(state: EpiphanyGraphsState): number {
  return state.architecture.nodes.length + state.architecture.edges.length + state.dataflow.nodes.length + state.dataflow.edges.length + state.links.length;
}

function codeRefLabel(ref: EpiphanyCodeRef): string {
  const line = ref.start_line ? `:${ref.start_line}` : "";
  const symbol = ref.symbol ? ` ${ref.symbol}` : "";
  return `${ref.path}${line}${symbol}`;
}

function useSnapshot() {
  const [snapshot, setSnapshot] = useState<OperatorSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionResult, setActionResult] = useState<OperatorActionResult | null>(null);
  const [runningAction, setRunningAction] = useState<OperatorAction | null>(null);
  const [request, setRequest] = useState<StatusRequest>({});

  async function refresh(nextRequest = request) {
    setLoading(true);
    setError(null);
    try {
      const result = await loadOperatorSnapshot(nextRequest);
      setSnapshot(result);
      const loadedThreadId = result.status?.threadId;
      const loadedState = result.status?.scene?.scene?.stateStatus;
      if (
        !nextRequest.threadId &&
        loadedState !== "missing" &&
        typeof loadedThreadId === "string" &&
        loadedThreadId.length > 0
      ) {
        setRequest((current) => (current.threadId ? current : { ...current, threadId: loadedThreadId }));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function runAction(action: OperatorAction) {
    setRunningAction(action);
    setError(null);
    setActionResult(null);
    try {
      const result = await runOperatorAction(action, request);
      setActionResult(result);
      const nextRequest = result.threadId ? { ...request, threadId: result.threadId } : request;
      if (result.threadId) {
        setRequest(nextRequest);
      }
      await refresh(nextRequest);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunningAction(null);
    }
  }

  useEffect(() => {
    void refresh({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { snapshot, loading, error, request, setRequest, refresh, actionResult, runningAction, runAction };
}

function useMidiHarmony() {
  const runtimeRef = useRef<HarmonyRuntime | null>(null);
  const [files, setFiles] = useState<MidiCorpusFile[]>([]);
  const [frame, setFrame] = useState<AquariumHarmonyFrame | null>(null);
  const [source, setSource] = useState<HarmonySource | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const installSource = useCallback((nextSource: HarmonySource) => {
    const runtime = createHarmonyRuntime(nextSource, harmonyAgentIds);
    runtimeRef.current = runtime;
    setSource(nextSource);
    setFrame(runtime.frame);
  }, []);

  const loadDefault = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const loaded = await loadShuffledDefaultHarmony(harmonyAgentIds);
      setFiles(loaded.files);
      installSource(loaded.source);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [installSource]);

  const nextSong = useCallback(async () => {
    if (!files.length) {
      await loadDefault();
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const loaded = await loadNextHarmony(files, harmonyAgentIds, source?.sourcePath);
      installSource(loaded.source);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [files, installSource, loadDefault, source?.sourcePath]);

  const changeFolder = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const loaded = await pickHarmonyFolder(harmonyAgentIds);
      setFiles(loaded.files);
      installSource(loaded.source);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [installSource]);

  useEffect(() => {
    const timeout = window.setTimeout(() => void loadDefault(), 4000);
    return () => window.clearTimeout(timeout);
  }, [loadDefault]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      const runtime = runtimeRef.current;
      if (runtime) {
        setFrame(runtime.next());
      }
    }, 1650);
    return () => window.clearInterval(interval);
  }, []);

  return { changeFolder, error, files, frame, loading, nextSong, source };
}

export function App() {
  const { snapshot, loading, error, request, setRequest, refresh, actionResult, runningAction, runAction } = useSnapshot();
  const harmony = useMidiHarmony();
  const [selectedCodeRef, setSelectedCodeRef] = useState<EpiphanyCodeRef | null>(null);
  const [activeDeck, setActiveDeck] = useState<DeckId>("command");
  const [subdeckByDeck, setSubdeckByDeck] = useState<Record<DeckId, string>>({
    command: "run",
    state: "environment",
    agents: "lanes",
    artifacts: "bundles",
  });
  const status = snapshot?.status;
  const scene = status?.scene?.scene ?? {};
  const pressure = status?.pressure?.pressure ?? {};
  const reorient = status?.reorient?.decision ?? {};
  const crrc = status?.crrc?.recommendation ?? {};
  const coordinator = status?.coordinator ?? {};
  const roles = useMemo(() => {
    const lanes = status?.roles?.roles;
    if (!Array.isArray(lanes)) return [];
    return [...lanes].sort((a, b) => roleOrder.indexOf(text(a.id)) - roleOrder.indexOf(text(b.id)));
  }, [status]);
  const jobs: any[] = Array.isArray(status?.jobs?.jobs) ? status.jobs.jobs : [];
  const planningResponse = status?.planning ?? {};
  const planningState = planningResponse?.planning ?? {};
  const planningSummary = planningResponse?.summary ?? {};
  const planningCaptures = objectList(planningState?.captures);
  const backlogItems = objectList(planningState?.backlog_items ?? planningState?.backlogItems);
  const roadmapStreams = objectList(planningState?.roadmap_streams ?? planningState?.roadmapStreams);
  const objectiveDrafts = objectList(planningState?.objective_drafts ?? planningState?.objectiveDrafts);
  const roleResults = status?.roleResults ?? {};
  const reorientResult = status?.reorientResult ?? {};
  const latestArtifact = snapshot?.artifacts?.[0];
  const latestImplementationArtifact = useMemo(
    () => (snapshot?.artifacts ?? []).find((artifact) => artifact.implementationAudit),
    [snapshot?.artifacts],
  );
  const latestImplementationAudit = latestImplementationArtifact?.implementationAudit;
  const latestRuntimeArtifact = useMemo(
    () => (snapshot?.artifacts ?? []).find((artifact) => artifact.runtimeAudit),
    [snapshot?.artifacts],
  );
  const latestRuntimeAudit = latestRuntimeArtifact?.runtimeAudit;
  const latestRiderArtifact = useMemo(
    () => (snapshot?.artifacts ?? []).find((artifact) => artifact.riderAudit),
    [snapshot?.artifacts],
  );
  const latestRiderAudit = latestRiderArtifact?.riderAudit;
  const implementationNoDiffPending =
    Boolean(latestArtifact?.implementationAudit) && latestArtifact?.implementationAudit?.workspaceChanged === false;
  const readyState = scene.stateStatus === "ready";
  const currentThreadId = request.threadId;
  const imaginationFinding = roleResults?.imagination?.finding;
  const canAcceptImagination =
    text(roleResults?.imagination?.status).toLowerCase() === "completed" &&
    Boolean(imaginationFinding?.statePatch?.planning);
  const modelingFinding = roleResults?.modeling?.finding;
  const canAcceptModeling =
    text(roleResults?.modeling?.status).toLowerCase() === "completed" && Boolean(modelingFinding?.statePatch);
  const canAcceptVerification = text(roleResults?.verification?.status).toLowerCase() === "completed";
  const canAcceptReorient = text(reorientResult?.status).toLowerCase() === "completed";
  const canContinueImplementation = text(coordinator.action).toLowerCase() === "continueimplementation";
  const selectedDraft = objectiveDrafts.find((draft) => text(draft.id, "") === request.planningDraftId);
  const selectedDraftStatus = text(selectedDraft?.status).toLowerCase();
  const canAdoptDraft =
    Boolean(selectedDraft) && !["adopted", "rejected", "superseded"].includes(selectedDraftStatus);
  const unityBridge = latestRuntimeAudit?.editorBridge;
  const installedEditors = latestRuntimeAudit?.installedEditors ?? [];
  const candidatePaths = latestRuntimeAudit?.candidatePaths ?? [];
  const searchRoots = latestRuntimeAudit?.searchRoots ?? [];
  const unityBridgeReady = latestRuntimeAudit?.status === "ready" && unityBridge?.exists === true;
  const epiphanyState = status?.read?.thread?.epiphanyState ?? status?.scene?.scene?.epiphanyState ?? {};
  const graphState = useMemo(() => normalizeGraphState(epiphanyState?.graphs), [epiphanyState]);
  const graphIssues = useMemo(() => validateEpiphanyGraphsState(graphState), [graphState]);
  const graphCount = graphRecordCount(graphState);
  const riderInstallations = latestRiderAudit?.installations ?? [];
  const riderSearchRoots = latestRiderAudit?.searchRoots ?? [];
  const riderChangedFiles = latestRiderAudit?.vcs?.changedFiles ?? [];
  const activeSubdeck = subdeckByDeck[activeDeck];
  const activeDeckTitle = deckLabels[activeDeck];
  const graphMenuActive = activeDeck === "state" && activeSubdeck === "graph";

  function selectDeck(deck: DeckId) {
    setActiveDeck(deck);
  }

  function selectSubdeck(deck: DeckId, subdeck: string) {
    setSubdeckByDeck((current) => ({ ...current, [deck]: subdeck }));
  }

  function actionBlocked(action: OperatorAction) {
    const button = actionButtons.find((item) => item.action === action);
    if (!button) return true;
    return Boolean(
      runningAction !== null ||
        (button.requiresThread && !currentThreadId) ||
        (button.requiresReadyState && !readyState) ||
        (button.requiresImaginationPatch && !canAcceptImagination) ||
        (button.requiresModelingPatch && !canAcceptModeling) ||
        (button.requiresVerificationResult && !canAcceptVerification) ||
        (button.requiresReorientResult && !canAcceptReorient) ||
        (button.requiresPlanningDraft && !canAdoptDraft) ||
        (button.requiresContinueImplementation && !canContinueImplementation) ||
        (button.requiresContinueImplementation && implementationNoDiffPending),
    );
  }

  function handleAquariumOption(option: AquariumOption) {
    if (option.deck) {
      selectDeck(option.deck);
      if (option.subdeck) {
        selectSubdeck(option.deck, option.subdeck);
      }
    }
    if (option.action && !actionBlocked(option.action)) {
      void runAction(option.action);
    }
  }

  const actionControls = actionButtons.map((button) => {
    const needsThread = button.requiresThread && !currentThreadId;
    const needsState = button.requiresReadyState && !readyState;
    const needsImagination = button.requiresImaginationPatch && !canAcceptImagination;
    const needsModeling = button.requiresModelingPatch && !canAcceptModeling;
    const needsVerification = button.requiresVerificationResult && !canAcceptVerification;
    const needsReorient = button.requiresReorientResult && !canAcceptReorient;
    const needsPlanningDraft = button.requiresPlanningDraft && !canAdoptDraft;
    const needsImplementation = button.requiresContinueImplementation && !canContinueImplementation;
    const needsNoDiffReview = button.requiresContinueImplementation && implementationNoDiffPending;
    const disabled =
      runningAction !== null ||
      needsThread ||
      needsState ||
      needsImagination ||
      needsModeling ||
      needsVerification ||
      needsReorient ||
      needsPlanningDraft ||
      needsImplementation ||
      needsNoDiffReview;
    const title = needsThread
      ? "Prepare a checkpoint or enter a persisted thread id first"
      : needsState
        ? "Prepare Epiphany state before launching this lane"
        : needsImagination
          ? "Read a completed imagination result with a planning patch before accepting it"
          : needsModeling
            ? "Read a completed modeling result with a state patch before accepting it"
            : needsVerification
              ? "Read a completed verification result before accepting it"
              : needsReorient
                ? "Read a completed reorient result before accepting it"
                : needsPlanningDraft
                  ? "Select a draft objective that has not already been adopted"
                  : needsImplementation
                    ? "Run the coordinator and clear review gates before continuing implementation"
                    : needsNoDiffReview
                      ? "Review the latest no-diff implementation artifact or run another lane before retrying"
                      : button.title;
    return (
      <button
        className="secondaryButton hudActionButton"
        onClick={() => void runAction(button.action)}
      disabled={disabled}
      title={title}
      data-interface-sound={disabled ? "action-disabled" : "action-primary"}
      key={button.action}
    >
        <ActionIcon icon={button.icon} />
        {runningAction === button.action ? button.runningLabel : button.label}
      </button>
    );
  });
  const aquariumPanelLines = (() => {
    if (activeDeck === "command" && activeSubdeck === "run") {
      return [
        text(actionResult?.summary, "Choose a bounded operation; review gates stay explicit."),
        text(actionResult?.artifactPath, "No fresh artifact path."),
      ];
    }
    if (activeDeck === "command" && activeSubdeck === "connection") {
      return [
        `Thread: ${text(status?.threadId)}`,
        `State: ${text(scene.stateStatus)} rev ${text(scene.revision)}`,
        `Repo: ${text(snapshot?.repoRoot)}`,
        `Draft: ${text(request.planningDraftId)}`,
      ];
    }
    if (activeDeck === "command" && activeSubdeck === "signals") {
      return [
        `Action: ${text(coordinator.action ?? crrc.action, "unknown")}`,
        `Target: ${text(coordinator.targetRole ?? crrc.recommendedSceneAction)}`,
        `Review: ${text(coordinator.requiresReview)}`,
        `Pressure: ${text(pressure.level)}`,
        `Reorient: ${text(reorient.action)} / ${text(reorient.nextAction)}`,
      ];
    }
    if (activeDeck === "state" && activeSubdeck === "environment") {
      return [
        `Unity: ${unityBridgeReady ? "bridge ready" : text(latestRuntimeAudit?.status, "unknown")}`,
        `Editor: ${text(latestRuntimeAudit?.editorPath, "missing")}`,
        `Rider: ${text(latestRiderAudit?.status, "unknown")}`,
        `Solution: ${text(latestRiderAudit?.solutionPath)}`,
        `Changed files: ${riderChangedFiles.length}`,
      ];
    }
    if (activeDeck === "state" && activeSubdeck === "planning") {
      return [
        `Captures: ${countText(planningSummary?.captureCount)} / pending ${countText(planningSummary?.pendingCaptureCount)}`,
        `Backlog: ${countText(planningSummary?.backlogItemCount)} / ready ${countText(planningSummary?.readyBacklogItemCount)}`,
        `Drafts: ${countText(planningSummary?.objectiveDraftCount)}`,
        text(selectedDraft?.title, "No draft objective selected."),
      ];
    }
    if (activeDeck === "state" && activeSubdeck === "graph") {
      return [
        `Architecture: ${graphState.architecture.nodes.length} nodes / ${graphState.architecture.edges.length} edges`,
        `Dataflow: ${graphState.dataflow.nodes.length} nodes / ${graphState.dataflow.edges.length} edges`,
        `Links: ${graphState.links.length}`,
        `Issues: ${graphIssues.length}`,
      ];
    }
    if (activeDeck === "agents" && activeSubdeck === "lanes") {
      return roles.slice(0, 8).map((role) => `${text(role.title)}: ${text(role.status)} / ${text(role.note)}`);
    }
    if (activeDeck === "agents" && activeSubdeck === "findings") {
      return [
        `Imagination: ${text(roleResults.imagination?.status)}`,
        `Modeling: ${text(roleResults.modeling?.status)}`,
        `Verification: ${text(roleResults.verification?.status)}`,
        `Reorient: ${text(reorientResult?.status)}`,
      ];
    }
    if (activeDeck === "agents" && activeSubdeck === "jobs") {
      return jobs.length ? jobs.slice(0, 8).map((job) => `${text(job.id)}: ${text(job.status)} / ${text(job.kind)}`) : ["No jobs loaded."];
    }
    return (snapshot?.artifacts ?? []).length
      ? (snapshot?.artifacts ?? []).slice(0, 8).map((artifact) => `${artifact.name}: ${artifact.files.length} files`)
      : ["No dogfood artifact bundles found."];
  })();
  const aquariumUi: AquariumUiFrame = {
    eyebrow: "Epiphany MVP",
    title: "Operator Console",
    reason: text(coordinator.reason ?? crrc.reason, "No recommendation loaded yet."),
    activeDeckLabel: activeDeckTitle,
    activeSubdeck,
    statuses: [
      { label: displayLabel(coordinator.action ?? crrc.action, "unknown"), tone: statusClass(coordinator.action ?? crrc.action) },
      { label: `pressure ${text(pressure.level, "unknown")}`, tone: statusClass(pressure.level) },
      { label: `continuity ${text(reorient.action, "unknown")}`, tone: statusClass(reorient.action) },
    ],
    deckButtons: (Object.keys(deckSubmenus) as DeckId[]).map((deck) => ({
      key: `ui:deck:${deck}`,
      label: deckLabels[deck],
      tone: activeDeck === deck ? "warn" : "neutral",
    })),
    subdeckButtons: deckSubmenus[activeDeck].map((subdeck) => ({
      key: `ui:subdeck:${activeDeck}:${subdeck}`,
      label: subdeck,
      tone: activeSubdeck === subdeck ? "warn" : "neutral",
    })),
    actionButtons:
      activeDeck === "command" && activeSubdeck === "run"
        ? actionButtons.map((button) => ({
            key: `ui:action:${button.action}`,
            label: runningAction === button.action ? button.runningLabel : button.label,
            disabled: actionBlocked(button.action),
            tone: actionBlocked(button.action) ? "neutral" : "ok",
          }))
        : [],
    panelTitle: `${activeDeckTitle} / ${activeSubdeck}`,
    panelLines: aquariumPanelLines,
    alert: error ?? (latestRuntimeAudit ? `Unity: ${text(latestRuntimeAudit.projectVersion)} is ${text(latestRuntimeAudit.status)}.` : undefined),
  };

  useEffect(() => {
    if (objectiveDrafts.length === 0) return;
    const draftIds = new Set(objectiveDrafts.map((draft) => text(draft.id, "")).filter(Boolean));
    setRequest((current) => {
      if (current.planningDraftId && draftIds.has(current.planningDraftId)) {
        return current;
      }
      const firstDraft =
        objectiveDrafts.find((draft) => text(draft.status).toLowerCase() === "draft") ?? objectiveDrafts[0];
      const firstDraftId = text(firstDraft?.id, "");
      return firstDraftId ? { ...current, planningDraftId: firstDraftId } : current;
    });
  }, [objectiveDrafts, setRequest]);

  const operatorSurface = (
    <>
      <header className="immersiveTopbar">
        <div className="operatorIdentity">
          <p className="eyebrow">Epiphany MVP</p>
          <h1>Operator Console</h1>
          <span>{text(coordinator.reason ?? crrc.reason, "No recommendation loaded yet.")}</span>
        </div>
        <div className="operatorTopControls">
          <Pill tone={statusClass(coordinator.action ?? crrc.action)}>
            {text(coordinator.action ?? crrc.action, "unknown")}
          </Pill>
          <Pill tone={statusClass(pressure.level)}>pressure {text(pressure.level, "unknown")}</Pill>
          <Pill tone={statusClass(reorient.action)}>continuity {text(reorient.action, "unknown")}</Pill>
          <button
            className="primaryButton"
            onClick={() => void refresh()}
            disabled={loading}
            title="Refresh status"
            data-interface-sound={loading ? "primary-disabled" : "primary-refresh"}
          >
            <RefreshCw size={16} aria-hidden="true" />
            {loading ? "Refreshing" : "Refresh"}
          </button>
          <PlaylistControl
            error={harmony.error}
            frame={harmony.frame}
            loading={harmony.loading}
            onChangeFolder={harmony.changeFolder}
            onNext={harmony.nextSong}
          />
        </div>
      </header>

      <nav className="deckRail" aria-label="Primary operator menus">
        {(Object.keys(deckSubmenus) as DeckId[]).map((deck) => (
          <button
            type="button"
            className={activeDeck === deck ? "active" : ""}
            onClick={() => selectDeck(deck)}
            data-interface-sound="deck-menu"
            key={deck}
          >
            {deck === "command" && <ClipboardCheck size={17} aria-hidden="true" />}
            {deck === "state" && <Map size={17} aria-hidden="true" />}
            {deck === "agents" && <BriefcaseBusiness size={17} aria-hidden="true" />}
            {deck === "artifacts" && <FileText size={17} aria-hidden="true" />}
            <span>{deckLabels[deck]}</span>
          </button>
        ))}
      </nav>

      <section className={`diegeticPanel ${graphMenuActive ? "widePanel" : ""}`} aria-label={`${activeDeckTitle} menu`}>
        <div className="deckHeader">
          <div>
            <span>{activeDeckTitle}</span>
            <h2>{activeSubdeck}</h2>
          </div>
          <div className="subdeckTabs" role="tablist" aria-label={`${activeDeckTitle} sections`}>
            {deckSubmenus[activeDeck].map((subdeck) => (
              <button
                type="button"
                className={activeSubdeck === subdeck ? "active" : ""}
                onClick={() => selectSubdeck(activeDeck, subdeck)}
                data-interface-sound="subdeck-menu"
                key={subdeck}
              >
                {subdeck}
              </button>
            ))}
          </div>
        </div>

        <div className="deckBody">
          {activeDeck === "command" && activeSubdeck === "run" && (
            <>
              <section className="hudActionGrid" aria-label="Bounded operator actions">
                {actionControls}
              </section>
              {actionResult && (
                <p className="actionResult hudResult">
                  {actionResult.summary} <code>{actionResult.artifactPath}</code>
                </p>
              )}
            </>
          )}

          {activeDeck === "command" && activeSubdeck === "connection" && (
            <section className="hudFormGrid" aria-label="Connection">
              <label>
                Thread ID
                <input
                  placeholder="auto-load persistent status thread"
                  value={request.threadId ?? ""}
                  onChange={(event) => setRequest({ ...request, threadId: event.target.value || undefined })}
                />
              </label>
              <label>
                Workspace
                <input
                  placeholder={snapshot?.repoRoot ?? "repo root"}
                  value={request.cwd ?? ""}
                  onChange={(event) => setRequest({ ...request, cwd: event.target.value || undefined })}
                />
              </label>
              <dl className="facts compact">
                <div><dt>Thread</dt><dd>{text(status?.threadId)}</dd></div>
                <div><dt>State</dt><dd>{text(scene.stateStatus)} rev {text(scene.revision)}</dd></div>
                <div><dt>Repo</dt><dd>{text(snapshot?.repoRoot)}</dd></div>
                <div><dt>Draft</dt><dd>{text(request.planningDraftId)}</dd></div>
              </dl>
            </section>
          )}

          {activeDeck === "command" && activeSubdeck === "signals" && (
            <section className="signalStack" aria-label="Coordinator and continuity">
              <div className={`actionBanner ${statusClass(coordinator.action ?? crrc.action)}`}>
                <strong>{text(coordinator.action ?? crrc.action, "unknown")}</strong>
                <span>{text(coordinator.targetRole ?? crrc.recommendedSceneAction)}</span>
              </div>
              <p className="reason">{text(coordinator.reason ?? crrc.reason, "No recommendation loaded yet.")}</p>
              <dl className="facts">
                <div><dt>Requires review</dt><dd>{text(coordinator.requiresReview)}</dd></div>
                <div><dt>Pressure</dt><dd><Pill tone={statusClass(pressure.level)}>{text(pressure.level)}</Pill></dd></div>
                <div><dt>Prepare compaction</dt><dd>{text(pressure.shouldPrepareCompaction)}</dd></div>
                <div><dt>Reorient</dt><dd><Pill tone={statusClass(reorient.action)}>{text(reorient.action)}</Pill></dd></div>
                <div><dt>Reasons</dt><dd>{listText(reorient.reasons)}</dd></div>
                <div><dt>Next</dt><dd>{text(reorient.nextAction)}</dd></div>
              </dl>
            </section>
          )}

          {activeDeck === "state" && activeSubdeck === "environment" && (
            <div className="environmentGrid hudEnvironmentGrid">
              <article className="environmentCard">
                <div className="cardTopline">
                  <h3>Unity Editor</h3>
                  <Pill tone={unityBridgeReady ? "ok" : statusClass(latestRuntimeAudit?.status)}>
                    {unityBridgeReady ? "bridge ready" : text(latestRuntimeAudit?.status, "unknown")}
                  </Pill>
                </div>
                <dl className="facts environmentFacts">
                  <div><dt>Project</dt><dd>{text(latestRuntimeAudit?.projectVersion)}</dd></div>
                  <div><dt>Editor</dt><dd>{text(latestRuntimeAudit?.editorPath, "missing")}</dd></div>
                  <div><dt>Package</dt><dd>{unityBridge?.exists ? "present" : "missing"}</dd></div>
                  <div><dt>Method</dt><dd>{text(unityBridge?.executeMethod)}</dd></div>
                </dl>
                {latestRuntimeAudit?.note && <p className="environmentNote">{latestRuntimeAudit.note}</p>}
                <PathList title="Installed" items={installedEditors.map((editor) => `${text(editor.version)} ${text(editor.editorPath)}`)} />
                <PathList title="Candidates" items={candidatePaths} />
              </article>

              <article className="environmentCard">
                <div className="cardTopline">
                  <h3>Rider</h3>
                  <Pill tone={statusClass(latestRiderAudit?.status)}>{text(latestRiderAudit?.status, "unknown")}</Pill>
                </div>
                <dl className="facts environmentFacts">
                  <div><dt>Workspace</dt><dd>{text(latestRiderAudit?.workspace ?? request.cwd ?? snapshot?.repoRoot)}</dd></div>
                  <div><dt>Solution</dt><dd>{text(latestRiderAudit?.solutionPath)}</dd></div>
                  <div><dt>Rider</dt><dd>{text(latestRiderAudit?.riderPath, "missing")}</dd></div>
                  <div><dt>Branch</dt><dd>{text(latestRiderAudit?.vcs?.branch)}</dd></div>
                  <div><dt>Dirty</dt><dd>{text(latestRiderAudit?.vcs?.dirty)}</dd></div>
                  <div><dt>Changed</dt><dd>{riderChangedFiles.length}</dd></div>
                </dl>
                <p className="environmentNote">{text(latestRiderAudit?.note, "Run Inspect Rider to capture source-context status.")}</p>
                <PathList title="Installations" items={riderInstallations.map((installation) => `${text(installation.versionHint)} ${text(installation.path)}`)} />
                <PathList title="Changed files" items={riderChangedFiles} />
                <PathList title="Search roots" items={riderSearchRoots} />
              </article>

              <article className="environmentCard">
                <div className="cardTopline">
                  <h3>Runtime Artifacts</h3>
                  <Pill tone={latestRuntimeArtifact ? "ok" : "neutral"}>{latestRuntimeArtifact ? "available" : "none"}</Pill>
                </div>
                <dl className="facts environmentFacts">
                  <div><dt>Runtime bundle</dt><dd>{text(latestRuntimeArtifact?.name)}</dd></div>
                  <div><dt>Files</dt><dd>{text(latestRuntimeArtifact?.files.length)}</dd></div>
                  <div><dt>Summary</dt><dd>{text(latestRuntimeArtifact?.summaryPath)}</dd></div>
                  <div><dt>Project path</dt><dd>{text(latestRuntimeAudit?.projectPath)}</dd></div>
                </dl>
                <PathList title="Search roots" items={searchRoots} />
                <code title={latestRuntimeArtifact?.path}>{text(latestRuntimeArtifact?.path)}</code>
              </article>
            </div>
          )}

          {activeDeck === "state" && activeSubdeck === "planning" && (
            <div className="planningGrid hudPlanningGrid">
              <article className="environmentCard planningSummary">
                <div className="cardTopline">
                  <h3>State</h3>
                  <Pill tone={statusClass(planningResponse?.stateStatus)}>
                    {text(planningResponse?.stateStatus, "missing")}
                  </Pill>
                </div>
                <dl className="facts environmentFacts">
                  <div><dt>Captures</dt><dd>{countText(planningSummary?.captureCount)}</dd></div>
                  <div><dt>Pending</dt><dd>{countText(planningSummary?.pendingCaptureCount)}</dd></div>
                  <div><dt>Backlog</dt><dd>{countText(planningSummary?.backlogItemCount)}</dd></div>
                  <div><dt>Ready</dt><dd>{countText(planningSummary?.readyBacklogItemCount)}</dd></div>
                  <div><dt>Streams</dt><dd>{countText(planningSummary?.roadmapStreamCount)}</dd></div>
                  <div><dt>Drafts</dt><dd>{countText(planningSummary?.objectiveDraftCount)}</dd></div>
                </dl>
                <label className="draftPicker">
                  Objective Draft
                  <select
                    value={request.planningDraftId ?? ""}
                    onChange={(event) =>
                      setRequest({ ...request, planningDraftId: event.target.value || undefined })
                    }
                    disabled={objectiveDrafts.length === 0}
                  >
                    <option value="">none</option>
                    {objectiveDrafts.map((draft) => (
                      <option value={text(draft.id, "")} key={text(draft.id)}>
                        {text(draft.title)} [{text(draft.status)}]
                      </option>
                    ))}
                  </select>
                </label>
                <PathList title="Roadmap" items={roadmapStreams.map((stream) => `${text(stream.id)}: ${text(stream.title)}`)} />
                {planningSummary?.note && <p className="environmentNote">{text(planningSummary.note)}</p>}
              </article>

              <div className="planningColumn">
                <div className="cardTopline planningColumnHeader">
                  <h3>Objective Drafts</h3>
                  <Pill tone={objectiveDrafts.length ? "warn" : "neutral"}>{objectiveDrafts.length}</Pill>
                </div>
                {objectiveDrafts.slice(0, 4).map((draft) => (
                  <PlanningItem
                    key={text(draft.id)}
                    title={text(draft.title)}
                    status={text(draft.status)}
                    selected={text(draft.id, "") === request.planningDraftId}
                    body={text(draft.summary)}
                    meta={[
                      text(draft.id),
                      `${
                        Array.isArray(draft.acceptance_criteria ?? draft.acceptanceCriteria)
                          ? (draft.acceptance_criteria ?? draft.acceptanceCriteria).length
                          : 0
                      } checks`,
                      listText(draft.source_item_ids ?? draft.sourceItemIds),
                    ]}
                  />
                ))}
                {objectiveDrafts.length === 0 && <EmptyState label="No objective drafts loaded." />}
              </div>

              <div className="planningColumn">
                <div className="cardTopline planningColumnHeader">
                  <h3>Backlog</h3>
                  <Pill tone={backlogItems.length ? "ok" : "neutral"}>{backlogItems.length}</Pill>
                </div>
                {backlogItems.slice(0, 4).map((item) => (
                  <PlanningItem
                    key={text(item.id)}
                    title={text(item.title)}
                    status={text(item.status)}
                    body={text(item.summary)}
                    meta={[text(item.priority?.value), text(item.horizon), text(item.product_area ?? item.productArea)]}
                  />
                ))}
                {backlogItems.length === 0 && <EmptyState label="No backlog items loaded." />}
              </div>

              <div className="planningColumn">
                <div className="cardTopline planningColumnHeader">
                  <h3>Captures</h3>
                  <Pill tone="neutral">{planningCaptures.length}</Pill>
                </div>
                {planningCaptures.slice(0, 4).map((capture) => {
                  const source = capture.source ?? {};
                  const sourceLabel =
                    source.repo && source.issue_number ? `${source.repo}#${source.issue_number}` : text(source.kind);
                  return (
                    <PlanningItem
                      key={text(capture.id)}
                      title={text(capture.title)}
                      status={text(capture.status)}
                      body={text(capture.body)}
                      meta={[text(capture.confidence), sourceLabel, listText(capture.tags)]}
                    />
                  );
                })}
                {planningCaptures.length === 0 && <EmptyState label="No captures loaded." />}
              </div>
            </div>
          )}

          {activeDeck === "state" && activeSubdeck === "graph" && (
            <section className="graphBand hudGraphBand">
              <div className="graphSummary">
                <dl className="facts environmentFacts">
                  <div><dt>Architecture</dt><dd>{graphState.architecture.nodes.length} nodes / {graphState.architecture.edges.length} edges</dd></div>
                  <div><dt>Dataflow</dt><dd>{graphState.dataflow.nodes.length} nodes / {graphState.dataflow.edges.length} edges</dd></div>
                  <div><dt>Links</dt><dd>{graphState.links.length}</dd></div>
                  <div><dt>Issues</dt><dd>{graphIssues.length}</dd></div>
                </dl>
                {selectedCodeRef && (
                  <div className="selectedCodeRef">
                    <Boxes size={16} aria-hidden="true" />
                    <span>Selected code ref</span>
                    <code title={codeRefLabel(selectedCodeRef)}>{codeRefLabel(selectedCodeRef)}</code>
                  </div>
                )}
              </div>
              {graphIssues.length > 0 && (
                <div className="graphIssues">
                  {graphIssues.slice(0, 4).map((issue) => (
                    <Pill tone="warn" key={`${issue.scope}:${issue.message}`}>{issue.scope}: {issue.message}</Pill>
                  ))}
                </div>
              )}
              {graphCount > 0 ? (
                <div className="graphViewerFrame">
                  <EpiphanyGraphViewer
                    state={graphState}
                    title="Epiphany Typed Graph"
                    className="embeddedGraphViewer"
                    style={{ minHeight: 520 }}
                    onCodeRefSelect={(codeRef) => setSelectedCodeRef(codeRef)}
                  />
                </div>
              ) : (
                <EmptyState label="No graph state loaded. Prepare a checkpoint or accept a modeling patch to grow the map." />
              )}
            </section>
          )}

          {activeDeck === "agents" && activeSubdeck === "lanes" && (
            <div className="cardGrid hudCardGrid">
              {roles.map((role) => (
                <article className="laneCard" key={text(role.id)}>
                  <div className="cardTopline">
                    <h3>{text(role.title)}</h3>
                    <Pill tone={statusClass(role.status)}>{text(role.status)}</Pill>
                  </div>
                  <p>{text(role.note)}</p>
                  <span className="owner">{text(role.ownerRole)}</span>
                </article>
              ))}
              {roles.length === 0 && <EmptyState label="No role lanes loaded." />}
            </div>
          )}

          {activeDeck === "agents" && activeSubdeck === "findings" && (
            <div className="stack">
              <Finding title="Imagination / Planning" result={roleResults.imagination} />
              <Finding title="Modeling / Checkpoint" result={roleResults.modeling} />
              <Finding title="Verification / Review" result={roleResults.verification} />
              <Finding title="Reorientation" result={reorientResult} findingKey="finding" />
            </div>
          )}

          {activeDeck === "agents" && activeSubdeck === "jobs" && (
            <div className="stack">
              {jobs.map((job) => (
                <article className="jobRow" key={text(job.id)}>
                  <div>
                    <strong>{text(job.id)}</strong>
                    <span>{text(job.kind)} - {text(job.ownerRole)}</span>
                  </div>
                  <Pill tone={statusClass(job.status)}>{text(job.status)}</Pill>
                </article>
              ))}
              {jobs.length === 0 && <EmptyState label="No jobs loaded." />}
            </div>
          )}

          {activeDeck === "artifacts" && activeSubdeck === "bundles" && (
            <div className="artifactTable" role="table" aria-label="Artifact bundles">
              <div className="artifactHeader" role="row">
                <span>Name</span>
                <span>Outcome</span>
                <span>Files</span>
                <span>Path</span>
              </div>
              {(snapshot?.artifacts ?? []).map((artifact: ArtifactBundle) => (
                <div className="artifactRow" role="row" key={artifact.path}>
                  <strong>{artifact.name}</strong>
                  <span><ArtifactOutcome artifact={artifact} /></span>
                  <span>{artifact.files.length}</span>
                  <code title={artifact.path}>{artifact.path}</code>
                </div>
              ))}
              {(snapshot?.artifacts ?? []).length === 0 && <EmptyState label="No dogfood artifact bundles found." />}
            </div>
          )}
        </div>
      </section>

      <aside className="hudToastStack" aria-label="Audit alerts">
        {error && (
          <section className="hudToast dangerNotice" role="alert">
            <AlertTriangle size={18} aria-hidden="true" />
            <span>{error}</span>
          </section>
        )}
        {latestImplementationAudit && (
          <section className={`hudToast ${latestImplementationAudit.workspaceChanged ? "okNotice" : "warnNotice"}`}>
            {latestImplementationAudit.workspaceChanged ? (
              <CheckCircle2 size={18} aria-hidden="true" />
            ) : (
              <AlertTriangle size={18} aria-hidden="true" />
            )}
            <span>
              <strong>Implementation:</strong>{" "}
              {latestImplementationAudit.workspaceChanged
                ? `${latestImplementationAudit.changedFiles.length} changed file(s).`
                : "no workspace diff; review before rerun."}
            </span>
          </section>
        )}
        {latestRuntimeAudit && (
          <section className={`hudToast ${latestRuntimeAudit.status === "ready" ? "okNotice" : "warnNotice"}`}>
            {latestRuntimeAudit.status === "ready" ? (
              <CheckCircle2 size={18} aria-hidden="true" />
            ) : (
              <AlertTriangle size={18} aria-hidden="true" />
            )}
            <span>
              <strong>Unity:</strong> {text(latestRuntimeAudit.projectVersion)} is {text(latestRuntimeAudit.status)}.
            </span>
          </section>
        )}
      </aside>
    </>
  );

  return (
    <main className="immersiveShell">
      <AgentConstellation
        roles={roles}
        roleResults={roleResults}
        reorientResult={reorientResult}
        coordinator={coordinator}
        crrc={crrc}
        pressure={pressure}
        reorient={reorient}
        jobs={jobs}
        variant="fullscreen"
        activeDeck={activeDeck}
        activeSubdeck={activeSubdeck}
        ui={aquariumUi}
        harmonyFrame={harmony.frame}
        operatorSurface={operatorSurface}
        onAgentOption={handleAquariumOption}
        isActionBlocked={actionBlocked}
      />
    </main>
  );
}

function Panel({ title, icon, children }: { title: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="panel">
      <SectionHeader title={title} icon={icon} />
      {children}
    </section>
  );
}

function PlaylistControl({
  error,
  frame,
  loading,
  onChangeFolder,
  onNext,
}: {
  error: string | null;
  frame: AquariumHarmonyFrame | null;
  loading: boolean;
  onChangeFolder: () => void | Promise<void>;
  onNext: () => void | Promise<void>;
}) {
  return (
    <details className="playlistControl">
      <summary title={frame?.sourcePath ?? error ?? "Loading classical MIDI harmony"} data-interface-sound="playlist-panel">
        <span>Harmony</span>
        <strong>{loading ? "loading" : frame?.chordLabel ?? "silent"}</strong>
      </summary>
      <div className="playlistBody">
        <p title={frame?.sourcePath ?? undefined}>{error ?? frame?.sourceName ?? "Finding a classical MIDI file."}</p>
        <div className="playlistButtons">
          <button type="button" onClick={() => void onNext()} disabled={loading} data-interface-sound="playlist-next">Shuffle Song</button>
          <button type="button" onClick={() => void onChangeFolder()} disabled={loading} data-interface-sound="playlist-folder">Folder</button>
        </div>
      </div>
    </details>
  );
}

function AgentConstellation({
  roles,
  roleResults,
  reorientResult,
  coordinator,
  crrc,
  pressure,
  reorient,
  jobs,
  variant = "band",
  activeDeck,
  activeSubdeck,
  ui,
  harmonyFrame,
  operatorSurface,
  onAgentOption,
  isActionBlocked,
}: {
  roles: any[];
  roleResults: any;
  reorientResult: any;
  coordinator: any;
  crrc: any;
  pressure: any;
  reorient: any;
  jobs: any[];
  variant?: "band" | "fullscreen";
  activeDeck?: DeckId;
  activeSubdeck?: string;
  ui?: AquariumUiFrame;
  harmonyFrame: AquariumHarmonyFrame | null;
  operatorSurface?: React.ReactNode;
  onAgentOption?: (option: AquariumOption) => void;
  isActionBlocked?: (action: OperatorAction) => boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const crispCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const optionByKeyRef = useRef(new globalThis.Map<string, AquariumOption>());
  const uiOptionByKeyRef = useRef(new globalThis.Map<string, AquariumOption>());
  const rendererRef = useRef<AquariumRenderer | null>(null);
  const agentNodeRefs = useRef(new globalThis.Map<string, HTMLButtonElement>());
  const thoughtNodeRefs = useRef(new globalThis.Map<string, HTMLDivElement>());
  const optionHaloNodeRefs = useRef(new globalThis.Map<string, HTMLDivElement>());
  const focusSurfaceRef = useRef<HTMLDivElement | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [hoveredAgentId, setHoveredAgentId] = useState<string | null>(null);
  const agents = useMemo<ProjectedAgent[]>(() => {
    return constellationSpecs.map((spec) => {
      const lane = roles.find((role) => text(role.id).toLowerCase() === spec.laneId);
      const result =
        roleResults?.[spec.laneId] ??
        (spec.id === "research" ? roleResults?.eyes ?? roleResults?.research : undefined);
      const ownedJobs = jobs.filter((job) => {
        const owner = text(job.ownerRole).toLowerCase();
        const kind = text(job.kind).toLowerCase();
        return owner.includes(spec.laneId) || owner.includes(spec.id) || kind.includes(spec.laneId);
      });
      let status = text(lane?.status ?? result?.status, "idle");
      let thought = projectedThought(
        lane?.note ?? findingSummary(result),
        "Quiet. Waiting for a bounded signal.",
      );
      let detail = text(lane?.ownerRole ?? result?.bindingId, spec.title);
      let review = result?.finding?.statePatch ? "patch review" : "none";

      if (spec.id === "coordinator") {
        status = text(coordinator?.action ?? crrc?.action, "unknown");
        thought = projectedThought(
          coordinator?.reason ?? crrc?.reason,
          "No coordinator projection loaded yet.",
        );
        detail = `target ${text(coordinator?.targetRole ?? crrc?.recommendedSceneAction, "none")}`;
        review = text(coordinator?.requiresReview, "false") === "true" ? "required" : "not required";
      } else if (spec.id === "reorientation") {
        status = text(lane?.status ?? reorient?.action ?? reorientResult?.status, "idle");
        thought = projectedThought(
          lane?.note ?? findingSummary(reorientResult) ?? reorient?.nextAction,
          "Continuity is quiet.",
        );
        detail = `pressure ${text(pressure?.level, "unknown")}`;
        review = text(reorientResult?.status).toLowerCase() === "completed" ? "read result" : "none";
      } else if (spec.id === "research" && !lane && !result) {
        status = "idle";
        thought = "Watching for proven paths before the machine invents one in a shed.";
        detail = "future lane";
        review = "none";
      }

      return {
        ...spec,
        status,
        tone: statusClass(status),
        thought,
        detail,
        activity: Math.min(projectedActivity(status, ownedJobs.length), 1),
        jobs: ownedJobs.length,
        review,
      };
    });
  }, [coordinator, crrc, jobs, pressure, reorient, reorientResult, roleResults, roles]);
  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? agents[0];
  const aquariumAgents = useMemo(() => {
    const optionsByKey = new globalThis.Map<string, AquariumOption>();
    const framedAgents = agents.map((agent) => {
      const options: AquariumOptionFrame[] = (aquariumOptionsByAgent[agent.id] ?? []).map((option, index) => {
        const key = `${agent.id}:${index}:${option.label}`;
        optionsByKey.set(key, option);
        return {
          key,
          label: option.label,
          disabled: option.action ? Boolean(isActionBlocked?.(option.action)) : false,
        };
      });
      return {
        ...agent,
        harmony: harmonyFrame?.agentVoices[agent.id],
        options,
      };
    });
    optionByKeyRef.current = optionsByKey;
    return framedAgents;
  }, [agents, harmonyFrame, isActionBlocked]);

  useEffect(() => {
    const uiOptions = new globalThis.Map<string, AquariumOption>();
    for (const button of ui?.deckButtons ?? []) {
      const deck = button.key.split(":")[2] as DeckId | undefined;
      if (deck && deck in deckLabels) {
        uiOptions.set(button.key, { label: button.label, deck });
      }
    }
    for (const button of ui?.subdeckButtons ?? []) {
      const [, , deck, subdeck] = button.key.split(":");
      if (deck && subdeck && deck in deckLabels) {
        uiOptions.set(button.key, { label: button.label, deck: deck as DeckId, subdeck });
      }
    }
    for (const button of ui?.actionButtons ?? []) {
      const action = button.key.split(":")[2] as OperatorAction | undefined;
      if (action) {
        uiOptions.set(button.key, { label: button.label, action });
      }
    }
    uiOptionByKeyRef.current = uiOptions;
  }, [ui]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const crispCanvas = crispCanvasRef.current;
    if (!canvas) return;
    const renderer = createAquariumRenderer(canvas, crispCanvas);
    rendererRef.current = renderer;
    return () => {
      renderer.dispose();
      rendererRef.current = null;
    };
  }, []);

  const bindAgentNode = useCallback((id: string, node: HTMLButtonElement | null) => {
    if (node) {
      agentNodeRefs.current.set(id, node);
    } else {
      agentNodeRefs.current.delete(id);
    }
  }, []);

  const bindThoughtNode = useCallback((id: string, node: HTMLDivElement | null) => {
    if (node) {
      thoughtNodeRefs.current.set(id, node);
    } else {
      thoughtNodeRefs.current.delete(id);
    }
  }, []);

  const bindOptionHaloNode = useCallback((id: string, node: HTMLDivElement | null) => {
    if (node) {
      optionHaloNodeRefs.current.set(id, node);
    } else {
      optionHaloNodeRefs.current.delete(id);
    }
  }, []);

  const applyProjectionFrame = useCallback((projections: AquariumAgentProjection[]) => {
    for (const projection of projections) {
      const agentNode = agentNodeRefs.current.get(projection.id);
      const thoughtNode = thoughtNodeRefs.current.get(projection.id);
      const optionHaloNode = optionHaloNodeRefs.current.get(projection.id);
      const focusSurfaceNode = (selectedAgentId ?? hoveredAgentId) === projection.id ? focusSurfaceRef.current : null;
      const properties: Array<[string, string]> = [
        ["--agent-x", `${projection.xPercent}%`],
        ["--agent-y", `${projection.yPercent}%`],
        ["--agent-tilt", `${projection.tilt}deg`],
        ["--agent-bubble-tilt", `${projection.tilt * 0.32}deg`],
        ["--agent-glow-pulse", String(projection.glowPulse)],
        ["--agent-glow-radius", `${14 + projection.glowPulse * 10}px`],
        ["--agent-hover-glow", `${projection.hover * (6 + projection.glowPulse * 8)}px`],
        ["--agent-expression", String(projection.expression)],
        ["--agent-hover", String(projection.hover)],
        ["--agent-ack", String(projection.acknowledgement)],
        ["--agent-scale", String(1 + projection.acknowledgement * 0.035 + projection.hover * 0.018)],
      ];
      for (const [name, value] of properties) {
        agentNode?.style.setProperty(name, value);
        thoughtNode?.style.setProperty(name, value);
        optionHaloNode?.style.setProperty(name, value);
        focusSurfaceNode?.style.setProperty(name, value);
      }
      thoughtNode?.toggleAttribute("data-agent-hot", projection.hover > 0.35);
      optionHaloNode?.toggleAttribute("data-agent-hot", projection.hover > 0.2);
    }
  }, [hoveredAgentId, selectedAgentId]);

  useEffect(() => {
    rendererRef.current?.setFrame({
      activeLabel: activeDeck ? `${deckLabels[activeDeck]} / ${activeSubdeck ?? ""}` : undefined,
      agents: aquariumAgents,
      onProjectionFrame: applyProjectionFrame,
      selectedAgentId: selectedAgentId ?? "",
      ui,
      variant,
    });
  }, [activeDeck, activeSubdeck, applyProjectionFrame, aquariumAgents, selectedAgentId, ui, variant]);

  function handlePointerMove(event: React.PointerEvent<HTMLCanvasElement>) {
    rendererRef.current?.setPointerClient(event.clientX, event.clientY);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLCanvasElement>) {
    event.currentTarget.setPointerCapture(event.pointerId);
    rendererRef.current?.pointerDownClient(event.clientX, event.clientY);
  }

  function handlePointerUp(event: React.PointerEvent<HTMLCanvasElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    rendererRef.current?.pointerUp();
  }

  function handlePointerLeave() {
    rendererRef.current?.clearPointer();
    rendererRef.current?.pointerUp();
  }

  function handleAgentPointerEnter(agentId: string, event: React.PointerEvent<HTMLElement>) {
    setHoveredAgentId(agentId);
    rendererRef.current?.setPointerClient(event.clientX, event.clientY);
    rendererRef.current?.setHoveredAgent(agentId);
  }

  function handleAgentPointerMove(event: React.PointerEvent<HTMLElement>) {
    rendererRef.current?.setPointerClient(event.clientX, event.clientY);
  }

  function handleAgentPointerLeave() {
    setHoveredAgentId(null);
    rendererRef.current?.setHoveredAgent(null);
  }

  function handleAgentPointerDown(agentId: string) {
    rendererRef.current?.wakeSoundscape();
    rendererRef.current?.acknowledgeAgent(agentId, "touch");
  }

  function handleOptionClick(event: React.MouseEvent<HTMLButtonElement>, optionKey: string) {
    event.stopPropagation();
    const option = optionByKeyRef.current.get(optionKey);
    if (option) {
      onAgentOption?.(option);
    }
  }

  function optionPositionStyle(index: number, count: number): React.CSSProperties {
    const arc = Math.min(Math.PI * 1.2, Math.max(Math.PI * 0.7, count * 0.34));
    const start = -Math.PI / 2 - arc / 2;
    const angle = start + (arc * (index + 0.5)) / Math.max(count, 1);
    const radius = 92;
    return {
      left: `calc(50% + ${Math.cos(angle) * radius}px)`,
      top: `calc(50% + ${Math.sin(angle) * radius}px)`,
    };
  }

  function handleInterfacePointerDown(event: React.PointerEvent<HTMLElement>) {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    if (target.closest(".agentCharacter")) return;
    const control = target.closest("button, summary, input, select, [data-interface-sound]");
    if (!(control instanceof HTMLElement)) return;
    const disabled =
      control instanceof HTMLButtonElement || control instanceof HTMLInputElement || control instanceof HTMLSelectElement
        ? control.disabled
        : false;
    const explicitKind = control.dataset.interfaceSound;
    const kind =
      explicitKind ??
      (control.closest(".diegeticPanel") ? "panel-control" : control.closest(".deckRail") ? "deck-menu" : "control");
    rendererRef.current?.wakeSoundscape();
    rendererRef.current?.triggerInterfaceHit(disabled ? `${kind}-disabled` : kind);
  }

  function handleCanvasClick() {
    const optionKey = rendererRef.current?.pickOption();
    if (optionKey) {
      const option = optionByKeyRef.current.get(optionKey) ?? uiOptionByKeyRef.current.get(optionKey);
      if (option) {
        onAgentOption?.(option);
      }
      return;
    }
    const agentId = rendererRef.current?.pickAgent();
    if (agentId) {
      setSelectedAgentId(agentId);
    } else {
      setSelectedAgentId(null);
    }
  }
  const focusedAgentId = selectedAgentId;
  const focusedAgent = focusedAgentId ? aquariumAgents.find((agent) => agent.id === focusedAgentId) : null;

  return (
    <section
      className={`${variant === "fullscreen" ? "immersiveConstellation" : "sectionBand agentConstellation"}`}
      aria-label="Agent state overview"
      onPointerDownCapture={() => rendererRef.current?.wakeSoundscape()}
      onKeyDownCapture={() => rendererRef.current?.wakeSoundscape()}
    >
      {variant === "band" && (
        <div className="constellationHeader">
          <SectionHeader title="Agent State" icon={<Boxes size={18} />} />
          <div className="constellationSignals" aria-label="Global signals">
            <Pill tone={statusClass(coordinator?.action ?? crrc?.action)}>
              {text(coordinator?.action ?? crrc?.action, "unknown")}
            </Pill>
            <Pill tone={statusClass(pressure?.level)}>pressure {text(pressure?.level, "unknown")}</Pill>
            <Pill tone={statusClass(reorient?.action)}>continuity {text(reorient?.action, "unknown")}</Pill>
          </div>
        </div>
      )}
      <div className="agentStage">
        <canvas
          ref={canvasRef}
          className="agentSmokeCanvas"
          aria-hidden="true"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerLeave={handlePointerLeave}
          onPointerUp={handlePointerUp}
          onClick={handleCanvasClick}
        />
        <canvas
          ref={crispCanvasRef}
          className="agentCrispCanvas"
          aria-hidden="true"
        />
        <div className="agentStageVignette" aria-hidden="true" />
        {operatorSurface && focusedAgent && (
          <div
            className={`agentFocusSurface ${selectedAgentId === focusedAgent.id ? "locked" : "preview"} ${
              focusedAgent.baseX > 62 ? "anchorLeft" : focusedAgent.baseX < 38 ? "anchorRight" : "anchorCenter"
            } ${focusedAgent.baseY > 58 ? "anchorUp" : "anchorDown"}`}
            ref={focusSurfaceRef}
            data-agent-focus={focusedAgent.id}
            onPointerDownCapture={handleInterfacePointerDown}
            style={
              {
                "--agent-x": `${focusedAgent.baseX}%`,
                "--agent-y": `${focusedAgent.baseY}%`,
                "--agent-color": focusedAgent.color,
                "--agent-glow": focusedAgent.glow,
              } as React.CSSProperties
            }
          >
            {operatorSurface}
          </div>
        )}
        {agents.map((agent) => (
          <button
            className={`agentCharacter ${agent.shape} ${agent.tone} ${selectedAgentId === agent.id ? "selected" : ""}`}
            key={agent.id}
            ref={(node) => bindAgentNode(agent.id, node)}
            type="button"
            data-agent-node={agent.id}
            onClick={() => {
              setSelectedAgentId(agent.id);
            }}
            onPointerEnter={(event) => handleAgentPointerEnter(agent.id, event)}
            onPointerDown={() => handleAgentPointerDown(agent.id)}
            onPointerMove={handleAgentPointerMove}
            onPointerLeave={handleAgentPointerLeave}
            onMouseEnter={(event) => {
              setHoveredAgentId(agent.id);
              rendererRef.current?.setPointerClient(event.clientX, event.clientY);
              rendererRef.current?.setHoveredAgent(agent.id);
            }}
            onMouseMove={(event) => rendererRef.current?.setPointerClient(event.clientX, event.clientY)}
            onMouseLeave={handleAgentPointerLeave}
            title={`${agent.name}: ${agent.thought}`}
            style={
              {
                "--agent-x": `${agent.baseX}%`,
                "--agent-y": `${agent.baseY}%`,
                "--agent-color": agent.color,
                "--agent-glow": agent.glow,
                "--agent-activity": agent.activity,
                "--agent-bubble-opacity": 0.38 + agent.activity * 0.28,
              } as React.CSSProperties
            }
          >
            <span className="agentGlyph" aria-hidden="true">{agent.glyph}</span>
            <span className="agentCaption">
              <strong>{agent.name}</strong>
              <span>{agent.status}</span>
            </span>
          </button>
        ))}
        {agents.map((agent) => (
          <div
            className={`thoughtBubble ${agent.tone} ${selectedAgentId === agent.id ? "selected" : ""}`}
            key={`${agent.id}-thought`}
            ref={(node) => bindThoughtNode(agent.id, node)}
            data-agent-thought={agent.id}
            style={
              {
                "--agent-x": `${agent.baseX}%`,
                "--agent-y": `${agent.baseY}%`,
                "--agent-color": agent.color,
                "--agent-glow": agent.glow,
                "--agent-activity": agent.activity,
                "--agent-bubble-opacity": 0.38 + agent.activity * 0.28,
              } as React.CSSProperties
            }
          >
            <strong>{agent.name}</strong>
            <span>{agent.thought}</span>
          </div>
        ))}
        {aquariumAgents.map((agent) => {
          const options = agent.options ?? [];
          if (!options.length) return null;
          const open = selectedAgentId === agent.id || hoveredAgentId === agent.id;
          return (
            <div
              className={`agentOptionHalo ${agent.tone} ${open ? "open" : ""}`}
              key={`${agent.id}-options`}
              ref={(node) => bindOptionHaloNode(agent.id, node)}
              data-agent-options={agent.id}
              onPointerEnter={(event) => {
                setHoveredAgentId(agent.id);
                rendererRef.current?.setPointerClient(event.clientX, event.clientY);
                rendererRef.current?.setHoveredAgent(agent.id);
              }}
              onPointerMove={(event) => rendererRef.current?.setPointerClient(event.clientX, event.clientY)}
              onPointerLeave={handleAgentPointerLeave}
              style={
                {
                  "--agent-x": `${agent.baseX}%`,
                  "--agent-y": `${agent.baseY}%`,
                  "--agent-color": agent.color,
                  "--agent-glow": agent.glow,
                } as React.CSSProperties
              }
            >
              {options.map((option, index) => (
                <button
                  type="button"
                  className="agentOptionButton"
                  key={option.key}
                  disabled={option.disabled}
                  title={option.label}
                  data-interface-sound={option.disabled ? "agent-option-disabled" : "agent-option"}
                  style={optionPositionStyle(index, options.length)}
                  onClick={(event) => handleOptionClick(event, option.key)}
                >
                  {option.label}
                </button>
              ))}
            </div>
          );
        })}
        {variant !== "fullscreen" && (
          <div className="constellationInspector">
            <div>
              <span>{selectedAgent.title}</span>
              <strong>{selectedAgent.name}</strong>
              <p>{selectedAgent.thought}</p>
            </div>
            <dl className="facts compact">
              <div><dt>Status</dt><dd><Pill tone={selectedAgent.tone}>{selectedAgent.status}</Pill></dd></div>
              <div><dt>Detail</dt><dd>{selectedAgent.detail}</dd></div>
              <div><dt>Jobs</dt><dd>{selectedAgent.jobs}</dd></div>
              <div><dt>Review</dt><dd>{selectedAgent.review}</dd></div>
            </dl>
          </div>
        )}
      </div>
    </section>
  );
}

function SectionHeader({ title, icon }: { title: string; icon: React.ReactNode }) {
  return (
    <div className="sectionHeader">
      {icon}
      <h2>{title}</h2>
    </div>
  );
}

function ActionIcon({ icon }: { icon: "file" | "check" | "play" | "eye" | "accept" | "runtime" | "plan" | "ide" }) {
  if (icon === "file") return <FileText size={16} aria-hidden="true" />;
  if (icon === "check") return <ClipboardCheck size={16} aria-hidden="true" />;
  if (icon === "play") return <Play size={16} aria-hidden="true" />;
  if (icon === "eye") return <Eye size={16} aria-hidden="true" />;
  if (icon === "runtime") return <Database size={16} aria-hidden="true" />;
  if (icon === "plan") return <ListChecks size={16} aria-hidden="true" />;
  if (icon === "ide") return <GitBranch size={16} aria-hidden="true" />;
  return <CheckCircle2 size={16} aria-hidden="true" />;
}

function Pill({ tone, children }: { tone: string; children: React.ReactNode }) {
  return <span className={`pill ${tone}`}>{children}</span>;
}

function Finding({ title, result, findingKey = "finding" }: { title: string; result: any; findingKey?: string }) {
  const finding = result?.[findingKey];
  return (
    <article className="findingCard">
      <div className="cardTopline">
        <h3>{title}</h3>
        <Pill tone={statusClass(result?.status)}>{text(result?.status)}</Pill>
      </div>
      {finding ? (
        <>
          <p>{text(finding.summary ?? finding.nextSafeMove ?? finding.mode ?? finding.verdict)}</p>
          <dl className="facts compact">
            <div><dt>Verdict</dt><dd>{text(finding.verdict ?? finding.mode)}</dd></div>
            <div><dt>Next</dt><dd>{text(finding.nextSafeMove)}</dd></div>
            <div><dt>Patch</dt><dd>{finding.statePatch ? "available" : "none"}</dd></div>
            <div><dt>Self Memory</dt><dd>{text(finding.selfPersistence?.status, "none")}</dd></div>
          </dl>
          {finding.selfPersistence?.reasons?.length ? (
            <p>{finding.selfPersistence.reasons.join("; ")}</p>
          ) : null}
        </>
      ) : (
        <p>{text(result?.note, "No finding available.")}</p>
      )}
    </article>
  );
}

function PlanningItem({
  title,
  status,
  body,
  meta,
  selected = false,
}: {
  title: string;
  status: string;
  body: string;
  meta: string[];
  selected?: boolean;
}) {
  const metaItems = meta.filter((item) => item && item !== "none");
  return (
    <article className={`planningItem ${selected ? "selected" : ""}`}>
      <div className="cardTopline">
        <h3>{title}</h3>
        <Pill tone={statusClass(status)}>{status}</Pill>
      </div>
      <p>{body}</p>
      {metaItems.length > 0 && <span className="planningMeta">{metaItems.join(" / ")}</span>}
    </article>
  );
}

function ArtifactOutcome({ artifact }: { artifact: ArtifactBundle }) {
  const audit = artifact.implementationAudit;
  const runtime = artifact.runtimeAudit;
  const rider = artifact.riderAudit;
  if (rider) {
    return (
      <Pill tone={rider.status === "ready" || rider.status === "captured" ? "ok" : "warn"}>
        Rider {rider.status}
      </Pill>
    );
  }
  if (runtime) {
    return (
      <Pill tone={runtime.status === "ready" ? "ok" : "warn"}>
        Unity {runtime.status}
      </Pill>
    );
  }
  if (!audit) return <span className="artifactOutcome muted">none</span>;
  return (
    <Pill tone={audit.workspaceChanged ? "ok" : "warn"}>
      {audit.workspaceChanged ? "Diff" : "No Diff"}
    </Pill>
  );
}

function PathList({ title, items }: { title: string; items: string[] }) {
  if (!items.length) return null;
  return (
    <div className="pathList">
      <strong>{title}</strong>
      {items.slice(0, 4).map((item) => (
        <code key={item} title={item}>{item}</code>
      ))}
      {items.length > 4 && <span>{items.length - 4} more</span>}
    </div>
  );
}

function EmptyState({ label }: { label: string }) {
  return <p className="emptyState">{label}</p>;
}
