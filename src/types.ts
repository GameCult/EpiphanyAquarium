export interface StatusRequest {
  threadId?: string;
  cwd?: string;
  codexHome?: string;
  appServer?: string;
  planningDraftId?: string;
}

export type OperatorAction =
  | "statusSnapshot"
  | "coordinatorPlan"
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
  status: any;
  artifacts: ArtifactBundle[];
}
