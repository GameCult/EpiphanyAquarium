export interface StatusRequest {
  memberId?: string;
  threadId?: string;
  cwd?: string;
  codexHome?: string;
  appServer?: string;
  planningDraftId?: string;
  targetMemberId?: string;
  communicationSubject?: string;
  communicationBody?: string;
  responseTo?: string;
}

export type OperatorAction =
  | "statusSnapshot"
  | "coordinatorPlan"
  | "heartbeatStatus"
  | "runHeartbeat"
  | "faceBubble"
  | "requestSwarmHelp"
  | "inspectUnity"
  | "inspectRider"
  | "prepareCheckpoint"
  | "continueImplementation"
  | "launchImagination"
  | "readImaginationResult"
  | "acceptImagination"
  | "launchModeling"
  | "readModelingResult"
  | "acceptModeling"
  | "launchVerification"
  | "readVerificationResult"
  | "acceptVerification"
  | "adoptObjectiveDraft"
  | "launchReorient"
  | "readReorientResult"
  | "acceptReorient";

export interface OperatorActionResult {
  action: OperatorAction;
  artifactPath: string;
  summary: string;
  threadId?: string;
}

export interface ArtifactBundle {
  name: string;
  path: string;
  files: string[];
  summaryPath?: string;
  finalStatusPath?: string;
  comparisonPath?: string;
  implementationAudit?: {
    resultPath: string;
    workspaceChanged: boolean;
    trackedDiffPresent: boolean;
    changedFiles: string[];
  };
  runtimeAudit?: {
    resultPath: string;
    status: string;
    projectPath?: string;
    projectVersion?: string;
    editorPath?: string;
    note?: string;
    editorBridge?: {
      exists: boolean;
      path?: string;
      relativePath?: string;
      executeMethod?: string;
    };
    installedEditors?: Array<{
      version?: string;
      editorPath?: string;
    }>;
    candidatePaths?: string[];
    searchRoots?: string[];
  };
  riderAudit?: {
    resultPath: string;
    status: string;
    workspace?: string;
    solutionPath?: string;
    solutionStatus?: string;
    riderPath?: string;
    installationCount?: number;
    note?: string;
    vcs?: {
      status?: string;
      branch?: string;
      dirty?: boolean;
      changedFiles?: string[];
      stagedFiles?: string[];
      changedRangesKnown?: boolean;
    };
    installations?: Array<{
      path?: string;
      versionHint?: string;
    }>;
    searchRoots?: string[];
  };
  modifiedMillis?: number;
}

export interface OperatorSnapshot {
  generatedAt: string;
  repoRoot: string;
  activeMember?: SwarmMember;
  swarmMembers?: SwarmMember[];
  status: any;
  artifacts: ArtifactBundle[];
  communications?: SwarmCommunication[];
}

export interface SwarmMember {
  id: string;
  label: string;
  kind: "harness" | "workspace";
  harnessRoot: string;
  workspaceRoot: string;
  stateRoot: string;
  codexHome: string;
  artifactRoot: string;
  description?: string;
  status?: string;
}

export interface SwarmCommunication {
  id: string;
  createdAt: string;
  fromMemberId: string;
  toMemberId: string;
  kind: "request" | "callback" | "note";
  status: "open" | "acknowledged" | "resolved" | "blocked";
  subject: string;
  body: string;
  blocker?: string;
  responseTo?: string;
  artifactPath?: string;
}
