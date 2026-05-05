use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusRequest {
    member_id: Option<String>,
    thread_id: Option<String>,
    cwd: Option<String>,
    codex_home: Option<String>,
    app_server: Option<String>,
    planning_draft_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwarmMember {
    id: String,
    label: String,
    kind: String,
    harness_root: PathBuf,
    workspace_root: PathBuf,
    state_root: PathBuf,
    codex_home: PathBuf,
    artifact_root: PathBuf,
    description: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperatorActionResult {
    action: String,
    artifact_path: String,
    summary: String,
    thread_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactBundle {
    name: String,
    path: String,
    files: Vec<String>,
    summary_path: Option<String>,
    final_status_path: Option<String>,
    comparison_path: Option<String>,
    implementation_audit: Option<ImplementationAudit>,
    runtime_audit: Option<RuntimeAudit>,
    rider_audit: Option<RiderAudit>,
    modified_millis: Option<u128>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImplementationAudit {
    result_path: String,
    workspace_changed: bool,
    tracked_diff_present: bool,
    changed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeAudit {
    result_path: String,
    status: String,
    project_path: Option<String>,
    project_version: Option<String>,
    editor_path: Option<String>,
    note: Option<String>,
    editor_bridge: Option<EditorBridgeAudit>,
    installed_editors: Vec<InstalledUnityEditor>,
    candidate_paths: Vec<String>,
    search_roots: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderAudit {
    result_path: String,
    status: String,
    workspace: Option<String>,
    solution_path: Option<String>,
    solution_status: Option<String>,
    rider_path: Option<String>,
    installation_count: Option<u64>,
    note: Option<String>,
    vcs: Option<RiderVcsAudit>,
    installations: Vec<RiderInstallation>,
    search_roots: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderVcsAudit {
    status: Option<String>,
    branch: Option<String>,
    dirty: Option<bool>,
    changed_files: Vec<String>,
    staged_files: Vec<String>,
    changed_ranges_known: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderInstallation {
    path: Option<String>,
    version_hint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorBridgeAudit {
    exists: bool,
    path: Option<String>,
    relative_path: Option<String>,
    execute_method: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledUnityEditor {
    version: Option<String>,
    editor_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperatorSnapshot {
    generated_at: String,
    repo_root: String,
    active_member: SwarmMember,
    swarm_members: Vec<SwarmMember>,
    status: Value,
    artifacts: Vec<ArtifactBundle>,
}

#[tauri::command]
fn load_operator_snapshot(request: Option<StatusRequest>) -> Result<OperatorSnapshot, String> {
    let request = request.unwrap_or_default();
    let swarm_members = load_swarm_members()?;
    let active_member = resolve_swarm_member(&swarm_members, request.member_id.as_deref())?;
    let status = load_status(&active_member, request)?;
    let artifacts = list_artifacts(&active_member)?;
    Ok(OperatorSnapshot {
        generated_at: unix_millis().to_string(),
        repo_root: active_member.harness_root.display().to_string(),
        active_member,
        swarm_members,
        status,
        artifacts,
    })
}

#[tauri::command]
fn run_operator_action(
    action: String,
    request: Option<StatusRequest>,
) -> Result<OperatorActionResult, String> {
    let request = request.unwrap_or_default();
    let swarm_members = load_swarm_members()?;
    let active_member = resolve_swarm_member(&swarm_members, request.member_id.as_deref())?;
    match action.as_str() {
        "statusSnapshot" => run_status_snapshot(&active_member, request),
        "coordinatorPlan" => run_coordinator_plan(&active_member, request),
        "inspectUnity" => run_unity_inspection(&active_member, request),
        "inspectRider" => run_rider_inspection(&active_member, request),
        "heartbeatStatus" | "runHeartbeat" | "faceBubble" => {
            run_gui_action_bridge(&active_member, request, action)
        }
        "launchImagination"
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
        | "acceptReorient"
        | "continueImplementation"
        | "prepareCheckpoint" => run_gui_action_bridge(&active_member, request, action),
        _ => Err(format!("unknown operator action: {action}")),
    }
}

fn run_rider_inspection(
    member: &SwarmMember,
    request: StatusRequest,
) -> Result<OperatorActionResult, String> {
    let python = find_python()?;
    let artifact_root = member.artifact_root.join("rider");
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(member.harness_root.join("tools").join("epiphany_rider_bridge.py"))
        .arg("status")
        .arg("--project-root")
        .arg(workspace)
        .arg("--artifact-root")
        .arg(artifact_root);
    let value = run_json_command(command, "rider inspection")?;
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Ok(OperatorActionResult {
        action: "inspectRider".to_string(),
        artifact_path: json_string(&value, "artifactPath")?,
        summary: format!("Rider bridge inspection: {status}."),
        thread_id: None,
    })
}

fn run_unity_inspection(
    member: &SwarmMember,
    request: StatusRequest,
) -> Result<OperatorActionResult, String> {
    let python = find_python()?;
    let artifact_root = member.artifact_root.join("runtime");
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(member.harness_root.join("tools").join("epiphany_unity_bridge.py"))
        .arg("inspect")
        .arg("--project-path")
        .arg(workspace)
        .arg("--artifact-root")
        .arg(artifact_root);
    let value = run_json_command(command, "unity inspection")?;
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Ok(OperatorActionResult {
        action: "inspectUnity".to_string(),
        artifact_path: json_string(&value, "artifactPath")?,
        summary: format!("Unity bridge inspection: {status}."),
        thread_id: None,
    })
}

fn load_status(member: &SwarmMember, request: StatusRequest) -> Result<Value, String> {
    let python = find_python()?;
    let status_script = member.harness_root.join("tools").join("epiphany_mvp_status.py");
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());
    let codex_home = request
        .codex_home
        .map(PathBuf::from)
        .unwrap_or_else(|| member.codex_home.clone());
    let transcript = member
        .artifact_root
        .join("status-transcript.jsonl");
    let stderr = member
        .artifact_root
        .join("status-server.stderr.log");

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(status_script)
        .arg("--json")
        .arg("--cwd")
        .arg(workspace)
        .arg("--codex-home")
        .arg(codex_home)
        .arg("--transcript")
        .arg(transcript)
        .arg("--stderr")
        .arg(stderr)
        .arg("--no-ephemeral");

    if let Some(thread_id) = request.thread_id {
        command.arg("--thread-id").arg(thread_id);
    }
    if let Some(app_server) = request.app_server {
        command.arg("--app-server").arg(app_server);
    }

    let output = command
        .output()
        .map_err(|err| format!("failed to run Epiphany status bridge: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Epiphany status bridge exited with {}: {}",
            output.status, stderr
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("failed to parse Epiphany status JSON: {err}"))
}

fn run_status_snapshot(
    member: &SwarmMember,
    request: StatusRequest,
) -> Result<OperatorActionResult, String> {
    let python = find_python()?;
    let artifact_root = member
        .artifact_root
        .join("status-snapshots")
        .join(unix_millis().to_string());
    fs::create_dir_all(&artifact_root)
        .map_err(|err| format!("failed to create status artifact dir: {err}"))?;
    let result_path = artifact_root.join("status.json");
    let transcript_path = artifact_root.join("transcript.jsonl");
    let stderr_path = artifact_root.join("server.stderr.log");
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());
    let codex_home = request
        .codex_home
        .map(PathBuf::from)
        .unwrap_or_else(|| member.codex_home.clone());

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(member.harness_root.join("tools").join("epiphany_mvp_status.py"))
        .arg("--json")
        .arg("--cwd")
        .arg(workspace)
        .arg("--codex-home")
        .arg(codex_home)
        .arg("--result")
        .arg(&result_path)
        .arg("--transcript")
        .arg(transcript_path)
        .arg("--stderr")
        .arg(stderr_path)
        .arg("--no-ephemeral");
    if let Some(thread_id) = request.thread_id {
        command.arg("--thread-id").arg(thread_id);
    }
    if let Some(app_server) = request.app_server {
        command.arg("--app-server").arg(app_server);
    }
    run_command(command, "status snapshot")?;
    Ok(OperatorActionResult {
        action: "statusSnapshot".to_string(),
        artifact_path: artifact_root.display().to_string(),
        summary: "Status snapshot written.".to_string(),
        thread_id: None,
    })
}

fn run_coordinator_plan(
    member: &SwarmMember,
    request: StatusRequest,
) -> Result<OperatorActionResult, String> {
    let python = find_python()?;
    let artifact_dir = member
        .artifact_root
        .join("coordinator")
        .join(format!("gui-coordinator-plan-{}", unix_millis()));
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());
    let codex_home = request
        .codex_home
        .map(PathBuf::from)
        .unwrap_or_else(|| member.codex_home.clone());

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(member.harness_root.join("tools").join("epiphany_mvp_coordinator.py"))
        .arg("--mode")
        .arg("plan")
        .arg("--max-steps")
        .arg("2")
        .arg("--cwd")
        .arg(workspace)
        .arg("--codex-home")
        .arg(codex_home)
        .arg("--artifact-dir")
        .arg(&artifact_dir);
    if let Some(thread_id) = request.thread_id {
        command.arg("--thread-id").arg(thread_id);
    }
    if let Some(app_server) = request.app_server {
        command.arg("--app-server").arg(app_server);
    }
    run_command(command, "coordinator plan")?;
    Ok(OperatorActionResult {
        action: "coordinatorPlan".to_string(),
        artifact_path: artifact_dir.display().to_string(),
        summary: "Coordinator plan artifact written.".to_string(),
        thread_id: None,
    })
}

fn run_gui_action_bridge(
    member: &SwarmMember,
    request: StatusRequest,
    action: String,
) -> Result<OperatorActionResult, String> {
    let thread_id = request.thread_id.clone();
    let python = find_python()?;
    let artifact_root = member.artifact_root.join("actions");
    let workspace = request
        .cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| member.workspace_root.clone());
    let codex_home = request
        .codex_home
        .map(PathBuf::from)
        .unwrap_or_else(|| member.codex_home.clone());

    let mut command = Command::new(python);
    command
        .current_dir(&member.harness_root)
        .arg(member.harness_root.join("tools").join("epiphany_gui_action.py"))
        .arg("--action")
        .arg(&action)
        .arg("--cwd")
        .arg(workspace)
        .arg("--codex-home")
        .arg(codex_home)
        .arg("--artifact-root")
        .arg(artifact_root);
    if let Some(thread_id) = thread_id {
        command.arg("--thread-id").arg(thread_id);
    }
    if let Some(app_server) = request.app_server {
        command.arg("--app-server").arg(app_server);
    }
    if let Some(planning_draft_id) = request.planning_draft_id {
        command.arg("--planning-draft-id").arg(planning_draft_id);
    }
    let value = run_json_command(command, &action)?;
    Ok(OperatorActionResult {
        action,
        artifact_path: json_string(&value, "artifactPath")?,
        summary: json_string(&value, "summary")?,
        thread_id: value
            .get("threadId")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn run_command(mut command: Command, label: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("{label} exited with {}: {}", output.status, stderr))
}

fn run_json_command(mut command: Command, label: &str) -> Result<Value, String> {
    let output = command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{label} exited with {}: {}", output.status, stderr));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("failed to parse {label} JSON: {err}"))
}

fn json_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string field in GUI action result: {key}"))
}

fn list_artifacts(member: &SwarmMember) -> Result<Vec<ArtifactBundle>, String> {
    let mut bundles = Vec::new();
    collect_artifact_root(&mut bundles, &member.artifact_root.join("coordinator"), "coordinator/")?;
    collect_artifact_root(&mut bundles, &member.artifact_root.join("dogfood"), "dogfood/")?;
    collect_artifact_root(
        &mut bundles,
        &member.artifact_root.join("actions"),
        "actions/",
    )?;
    collect_artifact_root(
        &mut bundles,
        &member.artifact_root.join("status-snapshots"),
        "status/",
    )?;
    collect_artifact_root(
        &mut bundles,
        &member.artifact_root.join("runtime"),
        "runtime/",
    )?;
    collect_artifact_root(
        &mut bundles,
        &member.artifact_root.join("rider"),
        "rider/",
    )?;
    if member.id == "epiphany-agent" {
        collect_artifact_root(&mut bundles, &member.harness_root.join(".epiphany-dogfood"), "")?;
    }

    bundles.sort_by(|a, b| b.modified_millis.cmp(&a.modified_millis));
    Ok(bundles)
}

fn collect_artifact_root(
    bundles: &mut Vec<ArtifactBundle>,
    root: &Path,
    name_prefix: &str,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&root).map_err(|err| format!("failed to read artifacts: {err}"))? {
        let entry = entry.map_err(|err| format!("failed to read artifact entry: {err}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        for file in
            fs::read_dir(&path).map_err(|err| format!("failed to read artifact bundle: {err}"))?
        {
            let file = file.map_err(|err| format!("failed to read artifact file: {err}"))?;
            if file.path().is_file() {
                files.push(file.file_name().to_string_lossy().to_string());
            }
        }
        files.sort();
        let raw_name = entry.file_name().to_string_lossy().to_string();
        let modified_millis = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_millis);
        bundles.push(ArtifactBundle {
            name: format!("{name_prefix}{raw_name}"),
            path: path.display().to_string(),
            summary_path: existing_path(&path, "epiphany-dogfood-summary.json")
                .or_else(|| existing_path(&path, "gui-action-summary.json"))
                .or_else(|| existing_path(&path, "unity-bridge-summary.json"))
                .or_else(|| existing_path(&path, "rider-bridge-summary.json"))
                .or_else(|| existing_path(&path, "status.json")),
            final_status_path: existing_path(&path, "epiphany-final-status.json")
                .or_else(|| existing_path(&path, "after-status.json")),
            comparison_path: existing_path(&path, "comparison.md"),
            implementation_audit: read_implementation_audit(&path),
            runtime_audit: read_runtime_audit(&path),
            rider_audit: read_rider_audit(&path),
            files,
            modified_millis,
        });
    }

    Ok(())
}

fn read_rider_audit(root: &Path) -> Option<RiderAudit> {
    let result_path = root.join("rider-bridge-summary.json");
    let text = fs::read_to_string(&result_path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let status = value.get("status").and_then(Value::as_str)?.to_string();
    Some(RiderAudit {
        result_path: result_path.display().to_string(),
        status,
        workspace: json_optional_string(&value, "workspace"),
        solution_path: json_optional_string(&value, "solutionPath"),
        solution_status: json_optional_string(&value, "solutionStatus"),
        rider_path: json_optional_string(&value, "riderPath"),
        installation_count: value.get("installationCount").and_then(Value::as_u64),
        note: json_optional_string(&value, "note"),
        vcs: read_rider_vcs(&value),
        installations: read_rider_installations(&value),
        search_roots: read_string_array(&value, "searchRoots"),
    })
}

fn read_rider_vcs(value: &Value) -> Option<RiderVcsAudit> {
    let vcs = value.get("vcs")?;
    Some(RiderVcsAudit {
        status: json_optional_string(vcs, "status"),
        branch: json_optional_string(vcs, "branch"),
        dirty: vcs.get("dirty").and_then(Value::as_bool),
        changed_files: read_string_array(vcs, "changedFiles"),
        staged_files: read_string_array(vcs, "stagedFiles"),
        changed_ranges_known: vcs.get("changedRangesKnown").and_then(Value::as_bool),
    })
}

fn read_rider_installations(value: &Value) -> Vec<RiderInstallation> {
    value
        .get("installations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| RiderInstallation {
                    path: json_optional_string(item, "path"),
                    version_hint: json_optional_string(item, "versionHint"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_runtime_audit(root: &Path) -> Option<RuntimeAudit> {
    let result_path = root.join("unity-bridge-summary.json");
    let text = fs::read_to_string(&result_path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let status = value.get("status").and_then(Value::as_str)?.to_string();
    Some(RuntimeAudit {
        result_path: result_path.display().to_string(),
        status,
        project_path: json_optional_string(&value, "projectPath"),
        project_version: json_optional_string(&value, "projectVersion"),
        editor_path: json_optional_string(&value, "editorPath"),
        note: json_optional_string(&value, "note"),
        editor_bridge: read_editor_bridge(&value),
        installed_editors: read_installed_editors(&value),
        candidate_paths: read_string_array(&value, "candidatePaths"),
        search_roots: read_string_array(&value, "searchRoots"),
    })
}

fn json_optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn read_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn read_editor_bridge(value: &Value) -> Option<EditorBridgeAudit> {
    let bridge = value.get("editorBridge")?;
    Some(EditorBridgeAudit {
        exists: bridge
            .get("exists")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        path: json_optional_string(bridge, "path"),
        relative_path: json_optional_string(bridge, "relativePath"),
        execute_method: json_optional_string(bridge, "executeMethod"),
    })
}

fn read_installed_editors(value: &Value) -> Vec<InstalledUnityEditor> {
    value
        .get("installedEditors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| InstalledUnityEditor {
                    version: json_optional_string(item, "version"),
                    editor_path: json_optional_string(item, "editorPath"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn existing_path(root: &Path, name: &str) -> Option<String> {
    let path = root.join(name);
    path.exists().then(|| path.display().to_string())
}

fn read_implementation_audit(root: &Path) -> Option<ImplementationAudit> {
    let result_path = root.join("implementation-result.json");
    let text = fs::read_to_string(&result_path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(ImplementationAudit {
        result_path: result_path.display().to_string(),
        workspace_changed: value
            .get("workspaceChanged")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        tracked_diff_present: value
            .get("trackedDiffPresent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        changed_files: value
            .get("changedFiles")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn load_swarm_members() -> Result<Vec<SwarmMember>, String> {
    let config_path = swarm_config_path()?;
    if config_path.exists() {
        let text = fs::read_to_string(&config_path)
            .map_err(|err| format!("failed to read swarm config {}: {err}", config_path.display()))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|err| format!("failed to parse swarm config {}: {err}", config_path.display()))?;
        let members = value
            .get("members")
            .and_then(Value::as_array)
            .ok_or_else(|| "swarm config missing members array".to_string())?;
        let parsed: Result<Vec<_>, _> = members
            .iter()
            .cloned()
            .map(serde_json::from_value::<SwarmMember>)
            .collect();
        let parsed = parsed.map_err(|err| format!("failed to decode swarm member: {err}"))?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    }
    default_swarm_members()
}

fn resolve_swarm_member(members: &[SwarmMember], member_id: Option<&str>) -> Result<SwarmMember, String> {
    if let Some(member_id) = member_id {
        if let Some(member) = members.iter().find(|member| member.id == member_id) {
            return Ok(member.clone());
        }
        return Err(format!("unknown swarm member: {member_id}"));
    }
    members
        .first()
        .cloned()
        .ok_or_else(|| "swarm registry has no members".to_string())
}

fn default_swarm_members() -> Result<Vec<SwarmMember>, String> {
    let harness_root = repo_root()?;
    let aetheria_root = harness_root
        .parent()
        .map(|path| path.join("AetheriaLore"))
        .unwrap_or_else(|| PathBuf::from(r"E:\Projects\AetheriaLore"));
    Ok(vec![
        SwarmMember {
            id: "epiphany-agent".to_string(),
            label: "Epiphany".to_string(),
            kind: "harness".to_string(),
            harness_root: harness_root.clone(),
            workspace_root: harness_root.clone(),
            state_root: harness_root.join("state"),
            codex_home: harness_root.join(".epiphany-gui").join("codex-home"),
            artifact_root: harness_root.join(".epiphany-gui"),
            description: Some("Main Epiphany harness instance.".to_string()),
            status: Some("active".to_string()),
        },
        SwarmMember {
            id: "aetheria-lore".to_string(),
            label: "Aetheria Lore".to_string(),
            kind: "workspace".to_string(),
            harness_root,
            workspace_root: aetheria_root.clone(),
            state_root: aetheria_root.join(".epiphany"),
            codex_home: aetheria_root.join(".epiphany").join("codex-home"),
            artifact_root: aetheria_root.join(".epiphany").join("artifacts"),
            description: Some("Aetheria vault and website swarm instance.".to_string()),
            status: Some("bootstrap".to_string()),
        },
    ])
}

fn swarm_config_path() -> Result<PathBuf, String> {
    Ok(aquarium_root()?.join(".epiphany-aquarium").join("swarm.json"))
}

fn aquarium_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed to locate Aquarium project root".to_string())
}

fn repo_root() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("EPIPHANY_AGENT_ROOT") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Ok(path);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(projects_root) = manifest_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    {
        let sibling_agent = projects_root.join("EpiphanyAgent");
        if sibling_agent.exists() {
            return Ok(sibling_agent);
        }
    }
    Err("failed to locate EpiphanyAgent; set EPIPHANY_AGENT_ROOT".to_string())
}

fn find_python() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("EPIPHANY_PYTHON") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Ok(path);
        }
    }
    let bundled = PathBuf::from(
        r"C:\Users\Meta\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe",
    );
    if bundled.exists() {
        return Ok(bundled);
    }
    Ok(PathBuf::from("python"))
}

fn unix_millis() -> u128 {
    system_time_millis(SystemTime::now()).unwrap_or_default()
}

fn system_time_millis(value: SystemTime) -> Option<u128> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_operator_snapshot,
            run_operator_action
        ])
        .run(tauri::generate_context!())
        .expect("error while running Epiphany operator");
}
