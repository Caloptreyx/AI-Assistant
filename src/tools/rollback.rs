//! Pre-change snapshot capture and inverse execution for reversible tools.
//!
//! Snapshots are stored as JSON in `dev_caloptreyx_aiassistant_snapshots` and replayed through the
//! same [`Access`] and [`Permissions`] gates as confirmed tool calls.

use super::{
    Ctx, Permissions, Scope,
    args::to_map,
    files,
    output::{request, request_json, sha256_hex},
};
use crate::upstream::Upstream;
use axum::http::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

/// Maximum size of a single file stored in a snapshot (1 MB).
pub const MAX_FILE_BYTES: usize = 1_048_576;
/// Maximum combined size of all files snapshotted in one `delete_files` batch (2 MB).
pub const MAX_DELETE_BATCH_BYTES: usize = 2_097_152;
/// Maximum directory listing pages scanned while capturing a `delete_files` snapshot.
const MAX_LIST_PAGES: i64 = 20;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletedFileSnapshot {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedEntry {
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupVarValue {
    pub env_variable: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Snapshot {
    WriteFile {
        server: String,
        file: String,
        existed: bool,
        previous_content: Option<String>,
        post_sha256: String,
    },
    EditFile {
        server: String,
        file: String,
        previous_content: String,
        post_sha256: String,
    },
    DeleteFiles {
        server: String,
        root: String,
        files: Vec<DeletedFileSnapshot>,
        skipped: Vec<SkippedEntry>,
    },
    UpdateStartupVariables {
        server: String,
        previous: Vec<StartupVarValue>,
        post: Vec<StartupVarValue>,
    },
    RenameServer {
        server: String,
        previous_name: String,
        previous_description: Option<String>,
        post_name: String,
        post_description: Option<String>,
    },
    CreateSchedule {
        server: String,
        schedule_uuid: String,
        name: String,
    },
}

impl Snapshot {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::WriteFile { .. } => "write_file",
            Self::EditFile { .. } => "edit_file",
            Self::DeleteFiles { .. } => "delete_files",
            Self::UpdateStartupVariables { .. } => "update_startup_variables",
            Self::RenameServer { .. } => "rename_server",
            Self::CreateSchedule { .. } => "create_schedule",
        }
    }

    pub fn required_server_permissions(&self, force: bool) -> Vec<&'static str> {
        let mut perms = Vec::new();
        match self {
            Self::WriteFile { existed, .. } => {
                perms.push(if *existed {
                    "files.create"
                } else {
                    "files.delete"
                });
                if !force {
                    perms.push("files.read-content");
                }
            }
            Self::EditFile { .. } | Self::DeleteFiles { .. } => {
                perms.push("files.create");
                if !force {
                    perms.push("files.read-content");
                }
            }
            Self::UpdateStartupVariables { .. } => {
                perms.push("startup.update");
                if !force {
                    perms.push("startup.read");
                }
            }
            Self::RenameServer { .. } => {
                perms.push("settings.rename");
            }
            Self::CreateSchedule { .. } => {
                perms.push("schedules.delete");
            }
        }
        perms
    }
}

/// Outcome of attempting to capture pre-change state for a confirmed action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RollbackCapture {
    Available {
        summary: String,
        snapshot: Snapshot,
    },
    Unavailable {
        summary: Option<String>,
    },
}

impl RollbackCapture {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            summary: Some(reason.into()),
        }
    }

    pub fn none() -> Self {
        Self::Unavailable { summary: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RollbackError {
    /// Target state changed after the action ran (only returned when `force == false`).
    Conflict(String),
    /// Permission denied, server scope violation, or panel error.
    Failed(String),
}

/// Joins a directory `root` and an entry `name` into a clean absolute file path.
pub fn join_path(root: &str, name: &str) -> String {
    let clean_root = root.trim().trim_end_matches('/');
    let clean_name = name.trim().trim_start_matches('/');
    if clean_root.is_empty() {
        format!("/{clean_name}")
    } else if clean_root.starts_with('/') {
        format!("{clean_root}/{clean_name}")
    } else {
        format!("/{clean_root}/{clean_name}")
    }
}

/// Splits a file path into `(root_directory, file_name)` for `/files/delete`.
pub fn split_file_path(path: &str) -> Result<(String, String), String> {
    let trimmed = path.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return Err(format!("Cannot delete root path \"{path}\"."));
    }
    match trimmed.rsplit_once('/') {
        Some(("", name)) => Ok(("/".to_string(), name.to_string())),
        Some((dir, name)) if !name.is_empty() => {
            let root = if dir.starts_with('/') {
                dir.to_string()
            } else {
                format!("/{dir}")
            };
            Ok((root, name.to_string()))
        }
        None => Ok(("/".to_string(), trimmed.to_string())),
        _ => Err(format!("Invalid file path \"{path}\".")),
    }
}

pub fn rename_summary(previous_name: &str, previous_description: Option<&str>) -> String {
    match previous_description {
        Some(desc) if !desc.is_empty() => {
            format!("Restore server name to \"{previous_name}\" and previous description")
        }
        _ => format!("Restore server name to \"{previous_name}\""),
    }
}

struct DirEntryInfo {
    is_file: bool,
    is_dir: bool,
    size: u64,
}

fn parse_dir_entry(entry: &Value) -> Option<(String, DirEntryInfo)> {
    let name = entry.get("name")?.as_str()?.to_string();
    let is_dir = entry
        .get("directory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_symlink = entry
        .get("symlink")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Calagopus Wings returns `file: bool`; also accept `is_file: bool` for compatibility.
    let is_file = entry
        .get("file")
        .and_then(Value::as_bool)
        .or_else(|| entry.get("is_file").and_then(Value::as_bool))
        .unwrap_or(false)
        && !is_dir
        && !is_symlink;
    let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
    Some((
        name,
        DirEntryInfo {
            is_file,
            is_dir,
            size,
        },
    ))
}

/// Captures UTF-8 contents of regular files about to be deleted by `delete_files`.
/// Never fails the primary delete action; degrades gracefully on errors or size caps.
pub async fn capture_delete_files(
    ctx: &Ctx,
    server: &str,
    root: &str,
    targets: &[&str],
) -> RollbackCapture {
    let mut entries: HashMap<String, DirEntryInfo> = HashMap::new();
    let mut page: i64 = 1;

    loop {
        let query = to_map(json!({
            "directory": root,
            "page": page,
            "per_page": 100,
        }));
        let parsed = match request_json(
            ctx,
            Method::GET,
            &format!("/api/client/servers/{server}/files/list"),
            Some(&query),
            None,
        )
        .await
        {
            Ok((_, Some(value))) => value,
            Ok((_, None)) => {
                return RollbackCapture::unavailable(
                    "Could not inspect directory listing before deleting.",
                );
            }
            Err(err) => {
                return RollbackCapture::unavailable(format!(
                    "Could not inspect directory listing before deleting: {err}"
                ));
            }
        };

        let list = parsed
            .pointer("/entries/data")
            .and_then(Value::as_array)
            .or_else(|| parsed.pointer("/data").and_then(Value::as_array));
        let Some(items) = list else {
            return RollbackCapture::unavailable(
                "Directory listing response did not include entries.",
            );
        };

        for item in items {
            if let Some((name, info)) = parse_dir_entry(item) {
                entries.insert(name, info);
            }
        }

        let all_found = targets
            .iter()
            .all(|name| entries.contains_key(name.trim_start_matches('/')));
        let total = parsed.pointer("/entries/total").and_then(Value::as_i64);
        let more = match total {
            Some(total) => page * 100 < total && !items.is_empty(),
            None => items.len() >= 100,
        };
        if all_found || !more || page >= MAX_LIST_PAGES {
            break;
        }
        page += 1;
    }

    let mut captured: Vec<DeletedFileSnapshot> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    let mut total_bytes: usize = 0;

    for raw_name in targets {
        let lookup = raw_name.trim_start_matches('/');
        let full_path = join_path(root, raw_name);

        let Some(info) = entries.get(lookup) else {
            skipped.push(SkippedEntry {
                name: (*raw_name).to_string(),
                reason: "not found in directory listing".into(),
            });
            continue;
        };

        if info.is_dir {
            skipped.push(SkippedEntry {
                name: (*raw_name).to_string(),
                reason: "directory (directories cannot be snapshotted)".into(),
            });
            continue;
        }

        if !info.is_file {
            skipped.push(SkippedEntry {
                name: (*raw_name).to_string(),
                reason: "not a regular file".into(),
            });
            continue;
        }

        if info.size > MAX_FILE_BYTES as u64 {
            skipped.push(SkippedEntry {
                name: (*raw_name).to_string(),
                reason: format!("exceeds 1 MB snapshot limit ({} bytes)", info.size),
            });
            continue;
        }

        if total_bytes.saturating_add(info.size as usize) > MAX_DELETE_BATCH_BYTES {
            skipped.push(SkippedEntry {
                name: (*raw_name).to_string(),
                reason: "exceeds 2 MB batch snapshot limit".into(),
            });
            continue;
        }

        match files::fetch(ctx, server, &full_path).await {
            Ok(Some(content)) => {
                if content.len() > MAX_FILE_BYTES {
                    skipped.push(SkippedEntry {
                        name: (*raw_name).to_string(),
                        reason: format!("exceeds 1 MB snapshot limit ({} bytes)", content.len()),
                    });
                } else if total_bytes.saturating_add(content.len()) > MAX_DELETE_BATCH_BYTES {
                    skipped.push(SkippedEntry {
                        name: (*raw_name).to_string(),
                        reason: "exceeds 2 MB batch snapshot limit".into(),
                    });
                } else {
                    total_bytes += content.len();
                    captured.push(DeletedFileSnapshot {
                        path: full_path,
                        content,
                    });
                }
            }
            Ok(None) => {
                skipped.push(SkippedEntry {
                    name: (*raw_name).to_string(),
                    reason: "file not found".into(),
                });
            }
            Err(err) => {
                skipped.push(SkippedEntry {
                    name: (*raw_name).to_string(),
                    reason: err,
                });
            }
        }
    }

    if captured.is_empty() {
        let reasons: Vec<String> = skipped
            .iter()
            .map(|item| format!("{}: {}", item.name, item.reason))
            .collect();
        let detail = if reasons.is_empty() {
            "No files could be snapshotted before deleting.".to_string()
        } else {
            format!("Could not snapshot deleted items ({}).", reasons.join("; "))
        };
        return RollbackCapture::unavailable(detail);
    }

    let summary = if skipped.is_empty() {
        format!(
            "Restore {} deleted file{}",
            captured.len(),
            if captured.len() == 1 { "" } else { "s" }
        )
    } else {
        let skipped_names: Vec<&str> = skipped.iter().map(|s| s.name.as_str()).collect();
        format!(
            "Restore {} of {} deleted items (skipped: {})",
            captured.len(),
            captured.len() + skipped.len(),
            skipped_names.join(", ")
        )
    };

    RollbackCapture::Available {
        summary,
        snapshot: Snapshot::DeleteFiles {
            server: server.to_string(),
            root: root.to_string(),
            files: captured,
            skipped,
        },
    }
}

/// Captures previous values of startup variables before `PUT /startup/variables`.
/// Never fails the primary update; degrades gracefully if `GET /startup/variables` fails.
pub async fn capture_startup_variables(
    ctx: &Ctx,
    server: &str,
    updates: &[(String, String)],
) -> RollbackCapture {
    let parsed = match request_json(
        ctx,
        Method::GET,
        &format!("/api/client/servers/{server}/startup/variables"),
        None,
        None,
    )
    .await
    {
        Ok((_, Some(value))) => value,
        Ok((_, None)) => {
            return RollbackCapture::unavailable(
                "Could not read previous startup variables before updating.",
            );
        }
        Err(err) => {
            return RollbackCapture::unavailable(format!(
                "Could not read previous startup variables before updating: {err}"
            ));
        }
    };

    let Some(list) = parsed.get("variables").and_then(Value::as_array) else {
        return RollbackCapture::unavailable(
            "Startup variables response did not include a variables list.",
        );
    };

    let mut current: HashMap<String, String> = HashMap::new();
    for item in list {
        if let (Some(name), Some(value)) = (
            item.get("env_variable").and_then(Value::as_str),
            item.get("value").and_then(Value::as_str),
        ) {
            current.insert(name.to_string(), value.to_string());
        }
    }

    let mut previous = Vec::with_capacity(updates.len());
    let mut post = Vec::with_capacity(updates.len());
    let mut missing = Vec::new();
    for (name, new_value) in updates {
        match current.get(name) {
            Some(old_value) => {
                previous.push(StartupVarValue {
                    env_variable: name.clone(),
                    value: old_value.clone(),
                });
                post.push(StartupVarValue {
                    env_variable: name.clone(),
                    value: new_value.clone(),
                });
            }
            None => missing.push(name.as_str()),
        }
    }

    if !missing.is_empty() || previous.is_empty() {
        return RollbackCapture::unavailable(format!(
            "Could not capture previous values for startup variable(s): {}.",
            missing.join(", ")
        ));
    }

    let names: Vec<&str> = previous.iter().map(|v| v.env_variable.as_str()).collect();
    let summary = format!(
        "Restore previous value{} for {}",
        if names.len() == 1 { "" } else { "s" },
        names.join(", ")
    );

    RollbackCapture::Available {
        summary,
        snapshot: Snapshot::UpdateStartupVariables {
            server: server.to_string(),
            previous,
            post,
        },
    }
}

/// Checks server permissions required to execute a rollback in `scope`.
fn check_permissions(
    scope: &Scope,
    permissions: &dyn Permissions,
    snapshot: &Snapshot,
    force: bool,
) -> Result<(), RollbackError> {
    if matches!(scope, Scope::Server(_)) {
        for perm in snapshot.required_server_permissions(force) {
            if !permissions.server(perm) {
                return Err(RollbackError::Failed(format!(
                    "You lack the {perm} permission required to roll back this action."
                )));
            }
        }
    }
    Ok(())
}

/// Reverts one captured snapshot through the panel API, re-checking permissions and server scope.
pub async fn execute(
    upstream: Arc<dyn Upstream>,
    scope: &Scope,
    permissions: &dyn Permissions,
    snapshot: &Snapshot,
    force: bool,
) -> Result<String, RollbackError> {
    check_permissions(scope, permissions, snapshot, force)?;
    let ctx = super::ctx(upstream, scope, true);

    match snapshot {
        Snapshot::WriteFile {
            server,
            file,
            existed,
            previous_content,
            post_sha256,
        } => {
            if !force {
                verify_file_hash(&ctx, server, file, post_sha256).await?;
            }
            if *existed {
                let prev = previous_content.as_deref().ok_or_else(|| {
                    RollbackError::Failed(format!(
                        "Snapshot for {file} is missing its previous contents."
                    ))
                })?;
                files::put(&ctx, server, file, prev)
                    .await
                    .map_err(RollbackError::Failed)?;
                Ok(format!(
                    "Restored previous contents of {file} ({} bytes) on server {server}.",
                    prev.len()
                ))
            } else {
                let (root, name) = split_file_path(file).map_err(RollbackError::Failed)?;
                request(
                    &ctx,
                    Method::POST,
                    &format!("/api/client/servers/{server}/files/delete"),
                    None,
                    Some(&json!({ "root": root, "files": [name] })),
                )
                .await
                .map_err(RollbackError::Failed)?;
                Ok(format!(
                    "Deleted newly created file {file} on server {server}."
                ))
            }
        }

        Snapshot::EditFile {
            server,
            file,
            previous_content,
            post_sha256,
        } => {
            if !force {
                verify_file_hash(&ctx, server, file, post_sha256).await?;
            }
            files::put(&ctx, server, file, previous_content)
                .await
                .map_err(RollbackError::Failed)?;
            Ok(format!(
                "Restored {file} to its previous contents ({} bytes) on server {server}.",
                previous_content.len()
            ))
        }

        Snapshot::DeleteFiles {
            server,
            root: _,
            files: deleted_files,
            skipped,
        } => {
            if !force {
                for entry in deleted_files {
                    match files::file_exists(&ctx, server, &entry.path).await {
                        Ok(true) => {
                            return Err(RollbackError::Conflict(format!(
                                "{} already exists on server {server}; rolling back would overwrite it.",
                                entry.path
                            )));
                        }
                        Ok(false) => {}
                        Err(err) => return Err(RollbackError::Failed(err)),
                    }
                }
            }
            for entry in deleted_files {
                files::put(&ctx, server, &entry.path, &entry.content)
                    .await
                    .map_err(RollbackError::Failed)?;
            }
            let restored_paths: Vec<&str> = deleted_files.iter().map(|f| f.path.as_str()).collect();
            let mut msg = format!(
                "Restored {} deleted file{} ({}) on server {server}.",
                deleted_files.len(),
                if deleted_files.len() == 1 { "" } else { "s" },
                restored_paths.join(", ")
            );
            if !skipped.is_empty() {
                let skipped_names: Vec<&str> = skipped.iter().map(|s| s.name.as_str()).collect();
                msg.push_str(&format!(
                    " Skipped items could not be restored: {}.",
                    skipped_names.join(", ")
                ));
            }
            Ok(msg)
        }

        Snapshot::UpdateStartupVariables {
            server,
            previous,
            post,
        } => {
            if !force {
                let (_, parsed) = request_json(
                    &ctx,
                    Method::GET,
                    &format!("/api/client/servers/{server}/startup/variables"),
                    None,
                    None,
                )
                .await
                .map_err(RollbackError::Failed)?;
                let list = parsed
                    .as_ref()
                    .and_then(|v| v.get("variables"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        RollbackError::Failed(
                            "Could not read current startup variables to verify rollback.".into(),
                        )
                    })?;
                let current: HashMap<&str, &str> = list
                    .iter()
                    .filter_map(|item| {
                        Some((
                            item.get("env_variable")?.as_str()?,
                            item.get("value")?.as_str()?,
                        ))
                    })
                    .collect();
                for expected in post {
                    match current.get(expected.env_variable.as_str()) {
                        Some(actual) if *actual == expected.value => {}
                        Some(actual) => {
                            return Err(RollbackError::Conflict(format!(
                                "Startup variable {} changed after this action (expected \"{}\", found \"{}\").",
                                expected.env_variable, expected.value, actual
                            )));
                        }
                        None => {
                            return Err(RollbackError::Conflict(format!(
                                "Startup variable {} is no longer present on server {server}.",
                                expected.env_variable
                            )));
                        }
                    }
                }
            }

            let variables: Vec<Value> = previous
                .iter()
                .map(|var| json!({ "env_variable": var.env_variable, "value": var.value }))
                .collect();
            request(
                &ctx,
                Method::PUT,
                &format!("/api/client/servers/{server}/startup/variables"),
                None,
                Some(&json!({ "variables": variables })),
            )
            .await
            .map_err(RollbackError::Failed)?;

            let names: Vec<&str> = previous.iter().map(|v| v.env_variable.as_str()).collect();
            Ok(format!(
                "Restored startup variable{} {} on server {server}. Restart the server for the restored values to take effect.",
                if names.len() == 1 { "" } else { "s" },
                names.join(", ")
            ))
        }

        Snapshot::RenameServer {
            server,
            previous_name,
            previous_description,
            post_name,
            post_description,
        } => {
            if !force {
                let (_, parsed) = request_json(
                    &ctx,
                    Method::GET,
                    &format!("/api/client/servers/{server}"),
                    None,
                    None,
                )
                .await
                .map_err(RollbackError::Failed)?;
                let current_name = parsed
                    .as_ref()
                    .and_then(|v| v.pointer("/server/name"))
                    .and_then(Value::as_str);
                let current_desc = parsed
                    .as_ref()
                    .and_then(|v| v.pointer("/server/description"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty());
                if current_name != Some(post_name.as_str())
                    || current_desc != post_description.as_deref().filter(|s| !s.is_empty())
                {
                    return Err(RollbackError::Conflict(format!(
                        "Server name or description changed after this action (current name: {:?}, expected: {:?}).",
                        current_name.unwrap_or(""),
                        post_name
                    )));
                }
            }

            let desc_value = match previous_description.as_deref().filter(|s| !s.is_empty()) {
                Some(desc) => json!(desc),
                None => Value::Null,
            };
            request(
                &ctx,
                Method::POST,
                &format!("/api/client/servers/{server}/settings/rename"),
                None,
                Some(&json!({
                    "name": previous_name,
                    "description": desc_value,
                })),
            )
            .await
            .map_err(RollbackError::Failed)?;

            Ok(format!(
                "Restored server {server} name to \"{previous_name}\"."
            ))
        }

        Snapshot::CreateSchedule {
            server,
            schedule_uuid,
            name,
        } => {
            request(
                &ctx,
                Method::DELETE,
                &format!("/api/client/servers/{server}/schedules/{schedule_uuid}"),
                None,
                None,
            )
            .await
            .map_err(RollbackError::Failed)?;

            Ok(format!(
                "Deleted schedule \"{name}\" ({schedule_uuid}) on server {server}."
            ))
        }
    }
}

async fn verify_file_hash(
    ctx: &Ctx,
    server: &str,
    file: &str,
    expected_sha256: &str,
) -> Result<(), RollbackError> {
    match files::fetch(ctx, server, file).await {
        Ok(Some(current)) => {
            let actual = sha256_hex(current.as_bytes());
            if actual.eq_ignore_ascii_case(expected_sha256) {
                Ok(())
            } else {
                Err(RollbackError::Conflict(format!(
                    "{file} was modified after this action (expected sha256 {expected_sha256}, found {actual})."
                )))
            }
        }
        Ok(None) => Err(RollbackError::Conflict(format!(
            "{file} no longer exists on server {server}."
        ))),
        Err(err) => {
            // Scope violations or permission errors should fail cleanly rather than look like an edit conflict.
            if err.contains("HTTP 403")
                || err.contains("HTTP 401")
                || err.contains("This chat belongs to server")
            {
                Err(RollbackError::Failed(err))
            } else {
                Err(RollbackError::Conflict(format!(
                    "Could not verify {file} before rolling back: {err}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        openapi::Index,
        tools::{self, Needs, Permissions, Scope},
    };
    use axum::{
        body::Body,
        http::{Method, Request, Response, header},
    };
    use std::{
        collections::{BTreeMap, HashSet},
        sync::Mutex,
    };

    const SERVER_A: &str = "123e4567-e89b-12d3-a456-426614174000";
    const SERVER_B: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    #[derive(Default)]
    struct MockState {
        files: BTreeMap<String, Vec<u8>>,
        dirs: HashSet<String>,
        vars: BTreeMap<String, String>,
        server_name: String,
        server_description: Option<String>,
        schedules: BTreeMap<String, String>,
        next_schedule_id: usize,
        fail_next_write: bool,
        calls: Vec<(Method, String)>,
    }

    struct StatefulPanel {
        state: Mutex<MockState>,
    }

    impl StatefulPanel {
        fn new() -> Arc<Self> {
            let mut state = MockState {
                server_name: "Survival".into(),
                server_description: Some("Main SMP".into()),
                ..Default::default()
            };
            state.vars.insert("MINECRAFT_VERSION".into(), "1.21.1".into());
            state.vars.insert("SERVER_JARFILE".into(), "server.jar".into());
            Arc::new(Self {
                state: Mutex::new(state),
            })
        }

        fn set_file(&self, path: &str, content: impl Into<Vec<u8>>) {
            self.state
                .lock()
                .unwrap()
                .files
                .insert(path.to_string(), content.into());
        }

        fn add_dir(&self, path: &str) {
            self.state.lock().unwrap().dirs.insert(path.to_string());
        }

        fn file_text(&self, path: &str) -> Option<String> {
            self.state
                .lock()
                .unwrap()
                .files
                .get(path)
                .map(|bytes| String::from_utf8(bytes.clone()).unwrap())
        }

        fn var(&self, name: &str) -> Option<String> {
            self.state.lock().unwrap().vars.get(name).cloned()
        }

        fn set_var(&self, name: &str, value: &str) {
            self.state
                .lock()
                .unwrap()
                .vars
                .insert(name.to_string(), value.to_string());
        }

        fn server_meta(&self) -> (String, Option<String>) {
            let s = self.state.lock().unwrap();
            (s.server_name.clone(), s.server_description.clone())
        }

        fn set_server_meta(&self, name: &str, desc: Option<&str>) {
            let mut s = self.state.lock().unwrap();
            s.server_name = name.to_string();
            s.server_description = desc.map(str::to_string);
        }

        fn schedule_exists(&self, uuid: &str) -> bool {
            self.state.lock().unwrap().schedules.contains_key(uuid)
        }

        fn fail_next_write(&self) {
            self.state.lock().unwrap().fail_next_write = true;
        }
    }

    fn query_param(uri: &axum::http::Uri, key: &str) -> Option<String> {
        let query = uri.query()?;
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=')
                && k == key
            {
                return Some(crate::access::percent_decode(&v.replace('+', " ")));
            }
        }
        None
    }

    fn json_response(status: u16, value: Value) -> Response<Body> {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }

    #[async_trait::async_trait]
    impl Upstream for StatefulPanel {
        async fn send(&self, request: Request<Body>) -> Result<Response<Body>, String> {
            let (parts, body) = request.into_parts();
            let path = parts.uri.path().to_string();
            let bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .map_err(|err| err.to_string())?;
            let mut state = self.state.lock().unwrap();
            state.calls.push((parts.method.clone(), path.clone()));

            if path.ends_with("/files/contents") && parts.method == Method::GET {
                let file = query_param(&parts.uri, "file").unwrap_or_default();
                return Ok(match state.files.get(&file) {
                    Some(content) => Response::builder()
                        .status(200)
                        .header(header::CONTENT_TYPE, "text/plain")
                        .body(Body::from(content.clone()))
                        .unwrap(),
                    None => json_response(404, json!({ "errors": ["file not found"] })),
                });
            }

            if path.ends_with("/files/write") && parts.method == Method::POST {
                if state.fail_next_write {
                    state.fail_next_write = false;
                    return Ok(json_response(500, json!({ "errors": ["disk full"] })));
                }
                let file = query_param(&parts.uri, "file").unwrap_or_default();
                state.files.insert(file, bytes.to_vec());
                return Ok(json_response(200, json!({})));
            }

            if path.ends_with("/files/list") && parts.method == Method::GET {
                let dir = query_param(&parts.uri, "directory").unwrap_or_else(|| "/".into());
                let dir_norm = dir.trim_end_matches('/');
                let mut data = Vec::new();
                for (file_path, content) in &state.files {
                    let (parent, name) = split_file_path(file_path).unwrap();
                    let parent_norm = parent.trim_end_matches('/');
                    if parent_norm == dir_norm {
                        data.push(json!({
                            "name": name,
                            "file": true,
                            "directory": false,
                            "symlink": false,
                            "size": content.len(),
                        }));
                    }
                }
                for dir_path in &state.dirs {
                    let (parent, name) = split_file_path(dir_path).unwrap();
                    let parent_norm = parent.trim_end_matches('/');
                    if parent_norm == dir_norm {
                        data.push(json!({
                            "name": name,
                            "file": false,
                            "directory": true,
                            "symlink": false,
                            "size": 0,
                        }));
                    }
                }
                let total = data.len();
                return Ok(json_response(
                    200,
                    json!({
                        "entries": { "total": total, "per_page": 100, "page": 1, "data": data }
                    }),
                ));
            }

            if path.ends_with("/files/delete") && parts.method == Method::POST {
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                let root = payload["root"].as_str().unwrap_or("/");
                for item in payload["files"].as_array().unwrap() {
                    let full = join_path(root, item.as_str().unwrap());
                    state.files.remove(&full);
                    state.dirs.remove(&full);
                }
                return Ok(json_response(200, json!({})));
            }

            if path.ends_with("/startup/variables") && parts.method == Method::GET {
                let vars: Vec<Value> = state
                    .vars
                    .iter()
                    .map(|(k, v)| json!({ "env_variable": k, "value": v }))
                    .collect();
                return Ok(json_response(200, json!({ "variables": vars })));
            }

            if path.ends_with("/startup/variables") && parts.method == Method::PUT {
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                for item in payload["variables"].as_array().unwrap() {
                    let name = item["env_variable"].as_str().unwrap().to_string();
                    let value = item["value"].as_str().unwrap().to_string();
                    state.vars.insert(name, value);
                }
                return Ok(json_response(200, json!({})));
            }

            if path.ends_with("/settings/rename") && parts.method == Method::POST {
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                if let Some(name) = payload.get("name").and_then(Value::as_str) {
                    state.server_name = name.to_string();
                }
                if let Some(desc) = payload.get("description") {
                    state.server_description = desc.as_str().map(str::to_string);
                }
                return Ok(json_response(200, json!({})));
            }

            if path == format!("/api/client/servers/{SERVER_A}") && parts.method == Method::GET {
                return Ok(json_response(
                    200,
                    json!({
                        "server": {
                            "name": state.server_name,
                            "description": state.server_description,
                        }
                    }),
                ));
            }

            if path.ends_with("/schedules") && parts.method == Method::POST {
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                state.next_schedule_id += 1;
                let id = format!("sched-{}", state.next_schedule_id);
                let name = payload["name"].as_str().unwrap_or("").to_string();
                state.schedules.insert(id.clone(), name);
                return Ok(json_response(200, json!({ "schedule": { "uuid": id } })));
            }

            if path.contains("/schedules/") && parts.method == Method::DELETE {
                let id = path.rsplit('/').next().unwrap();
                return Ok(if state.schedules.remove(id).is_some() {
                    json_response(200, json!({}))
                } else {
                    json_response(404, json!({ "errors": ["schedule not found"] }))
                });
            }

            Ok(json_response(404, json!({ "errors": ["not found"] })))
        }

        async fn openapi(&self) -> Result<Arc<Index>, String> {
            Ok(Arc::new(Index::new(&json!({ "paths": {} }))))
        }
    }

    struct Grants {
        server: Vec<&'static str>,
    }

    impl Grants {
        fn server(perms: &[&'static str]) -> Self {
            Self {
                server: perms.to_vec(),
            }
        }

        fn everything() -> Self {
            let mut server = vec!["schedules.delete"];
            for tool in tools::all() {
                match tool.needs {
                    Needs::Server(list) | Needs::AnyServer(list) => server.extend(list),
                    Needs::Nothing | Needs::Admin(_) => {}
                }
            }
            Self { server }
        }
    }

    impl Permissions for Grants {
        fn server(&self, permission: &str) -> bool {
            self.server.contains(&permission)
        }

        fn admin(&self, _permission: &str) -> bool {
            true
        }
    }

    fn scope_a() -> Scope {
        Scope::Server(SERVER_A.into())
    }

    fn expect_snapshot(capture: RollbackCapture) -> Snapshot {
        match capture {
            RollbackCapture::Available { snapshot, .. } => snapshot,
            other => panic!("expected Available snapshot, got {other:?}"),
        }
    }

    #[test]
    fn path_helpers_handle_root_and_nested_paths() {
        assert_eq!(join_path("/", "eula.txt"), "/eula.txt");
        assert_eq!(join_path("/logs", "old.log"), "/logs/old.log");
        assert_eq!(join_path("/logs/", "/old.log"), "/logs/old.log");

        assert_eq!(
            split_file_path("/eula.txt").unwrap(),
            ("/".into(), "eula.txt".into())
        );
        assert_eq!(
            split_file_path("eula.txt").unwrap(),
            ("/".into(), "eula.txt".into())
        );
        assert_eq!(
            split_file_path("/config/settings.yml").unwrap(),
            ("/config".into(), "settings.yml".into())
        );
        assert!(split_file_path("/").is_err());
    }

    #[tokio::test]
    async fn write_file_create_and_overwrite_rollback() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();

        // 1. Create a new file -> rollback deletes it.
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "write_file",
            &json!({ "file": "/eula.txt", "content": "eula=true\n" }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(panel.file_text("/eula.txt").as_deref(), Some("eula=true\n"));
        let snap = expect_snapshot(capture);

        let res = execute(panel.clone(), &scope_a(), &grants, &snap, false).await;
        assert!(res.is_ok(), "{res:?}");
        assert_eq!(panel.file_text("/eula.txt"), None);

        // 2. Overwrite an existing file -> rollback restores previous contents.
        panel.set_file("/server.properties", "max-players=20\n");
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "write_file",
            &json!({ "file": "/server.properties", "content": "max-players=50\n" }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            panel.file_text("/server.properties").as_deref(),
            Some("max-players=50\n")
        );
        let snap = expect_snapshot(capture);

        let res = execute(panel.clone(), &scope_a(), &grants, &snap, false).await;
        assert!(res.is_ok(), "{res:?}");
        assert_eq!(
            panel.file_text("/server.properties").as_deref(),
            Some("max-players=20\n")
        );
    }

    #[tokio::test]
    async fn edit_file_and_multi_edit_chain_rollback_without_false_conflict() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();
        panel.set_file("/server.properties", "difficulty=easy\npvp=false\n");

        // Edit 1: easy -> normal
        let (o1, c1) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "edit_file",
            &json!({ "file": "/server.properties", "old_string": "difficulty=easy", "new_string": "difficulty=normal" }),
        )
        .await;
        assert!(o1.is_ok(), "{o1:?}");
        let s1 = expect_snapshot(c1);

        // Edit 2: pvp=false -> pvp=true
        let (o2, c2) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "edit_file",
            &json!({ "file": "/server.properties", "old_string": "pvp=false", "new_string": "pvp=true" }),
        )
        .await;
        assert!(o2.is_ok(), "{o2:?}");
        let s2 = expect_snapshot(c2);

        // Rolling back Edit 1 out-of-order detects conflict because Edit 2 modified the file.
        let conflict = execute(panel.clone(), &scope_a(), &grants, &s1, false).await;
        assert!(matches!(conflict, Err(RollbackError::Conflict(_))));

        // Rolling back Edit 2 first, then Edit 1 succeeds cleanly with no false conflict!
        execute(panel.clone(), &scope_a(), &grants, &s2, false)
            .await
            .unwrap();
        assert_eq!(
            panel.file_text("/server.properties").as_deref(),
            Some("difficulty=normal\npvp=false\n")
        );

        execute(panel.clone(), &scope_a(), &grants, &s1, false)
            .await
            .unwrap();
        assert_eq!(
            panel.file_text("/server.properties").as_deref(),
            Some("difficulty=easy\npvp=false\n")
        );
    }

    #[tokio::test]
    async fn file_conflict_detection_and_force_override() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();
        panel.set_file("/config.yml", "port: 25565\n");

        let (_, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "edit_file",
            &json!({ "file": "/config.yml", "old_string": "25565", "new_string": "25566" }),
        )
        .await;
        let snap = expect_snapshot(capture);

        // Simulate a manual user edit after the AI action.
        panel.set_file("/config.yml", "port: 25570\n");

        let err = execute(panel.clone(), &scope_a(), &grants, &snap, false)
            .await
            .unwrap_err();
        assert!(matches!(err, RollbackError::Conflict(_)), "{err:?}");
        assert_eq!(panel.file_text("/config.yml").as_deref(), Some("port: 25570\n"));

        // Force rollback overwrites the manual edit.
        execute(panel.clone(), &scope_a(), &grants, &snap, true)
            .await
            .unwrap();
        assert_eq!(panel.file_text("/config.yml").as_deref(), Some("port: 25565\n"));
    }

    #[tokio::test]
    async fn delete_files_captures_regular_files_skips_directories_and_detects_recreation() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();
        panel.set_file("/logs/old.log", "line 1\n");
        panel.add_dir("/logs/archive");

        // Partial capture: regular file captured, directory skipped.
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "delete_files",
            &json!({ "root": "/logs", "files": ["old.log", "archive"] }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(panel.file_text("/logs/old.log"), None);

        let summary = match &capture {
            RollbackCapture::Available { summary, .. } => summary.clone(),
            other => panic!("expected Available, got {other:?}"),
        };
        assert!(summary.contains("skipped: archive"), "{summary}");
        let snap = expect_snapshot(capture);

        // Recreating the file triggers a conflict unless forced.
        panel.set_file("/logs/old.log", "new content");
        let conflict = execute(panel.clone(), &scope_a(), &grants, &snap, false).await;
        assert!(matches!(conflict, Err(RollbackError::Conflict(_))));

        // Force restores the deleted file.
        execute(panel.clone(), &scope_a(), &grants, &snap, true)
            .await
            .unwrap();
        assert_eq!(panel.file_text("/logs/old.log").as_deref(), Some("line 1\n"));

        // Deleting only a directory degrades gracefully to Unavailable.
        panel.add_dir("/world_backup");
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "delete_files",
            &json!({ "root": "/", "files": ["world_backup"] }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        match capture {
            RollbackCapture::Unavailable { summary: Some(reason) } => {
                assert!(reason.contains("directory"), "{reason}");
            }
            other => panic!("expected Unavailable with directory reason, got {other:?}"),
        }

        // Batch cap (2 MB): three 800 KB files -> first two snapshotted, third skipped.
        let chunk = "x".repeat(800_000);
        panel.set_file("/batch/f1.txt", chunk.clone());
        panel.set_file("/batch/f2.txt", chunk.clone());
        panel.set_file("/batch/f3.txt", chunk);
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "delete_files",
            &json!({ "root": "/batch", "files": ["f1.txt", "f2.txt", "f3.txt"] }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        match expect_snapshot(capture) {
            Snapshot::DeleteFiles { files, skipped, .. } => {
                assert_eq!(files.len(), 2);
                assert_eq!(skipped.len(), 1);
                assert_eq!(skipped[0].name, "f3.txt");
                assert!(skipped[0].reason.contains("2 MB"), "{}", skipped[0].reason);
            }
            other => panic!("expected DeleteFiles snapshot, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn oversized_files_degrade_gracefully_without_failing_primary_action() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();
        let big = "a".repeat(MAX_FILE_BYTES + 10);
        let big_hash = sha256_hex(big.as_bytes());
        panel.set_file("/huge.txt", big.clone());

        // write_file over a >1 MB file succeeds, but rollback is Unavailable.
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "write_file",
            &json!({ "file": "/huge.txt", "content": "small", "expected_sha256": big_hash }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(panel.file_text("/huge.txt").as_deref(), Some("small"));
        match capture {
            RollbackCapture::Unavailable { summary: Some(reason) } => {
                assert!(reason.contains("1 MB"), "{reason}");
            }
            other => panic!("expected Unavailable with size reason, got {other:?}"),
        }

        // edit_file on a >1 MB file also succeeds and degrades gracefully.
        let big_edit = format!("{}MARKER", "b".repeat(MAX_FILE_BYTES));
        panel.set_file("/huge2.txt", big_edit);
        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "edit_file",
            &json!({ "file": "/huge2.txt", "old_string": "MARKER", "new_string": "DONE" }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(matches!(
            capture,
            RollbackCapture::Unavailable { summary: Some(_) }
        ));
    }

    #[tokio::test]
    async fn failed_primary_action_never_stores_snapshot() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();
        panel.set_file("/a.txt", "old");
        panel.fail_next_write();

        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "write_file",
            &json!({ "file": "/a.txt", "content": "new" }),
        )
        .await;
        assert!(outcome.is_err());
        assert_eq!(capture, RollbackCapture::none());
    }

    #[tokio::test]
    async fn startup_variables_rollback_and_conflict() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();

        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "update_startup_variables",
            &json!({ "variables": { "MINECRAFT_VERSION": "1.21.4" } }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(panel.var("MINECRAFT_VERSION").as_deref(), Some("1.21.4"));
        let snap = expect_snapshot(capture);

        // External modification causes conflict.
        panel.set_var("MINECRAFT_VERSION", "1.21.5");
        let conflict = execute(panel.clone(), &scope_a(), &grants, &snap, false).await;
        assert!(matches!(conflict, Err(RollbackError::Conflict(_))));

        // Force overrides conflict and restores original value.
        execute(panel.clone(), &scope_a(), &grants, &snap, true)
            .await
            .unwrap();
        assert_eq!(panel.var("MINECRAFT_VERSION").as_deref(), Some("1.21.1"));
    }

    #[tokio::test]
    async fn rename_server_rollback_and_conflict() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();

        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "rename_server",
            &json!({ "name": "Creative", "description": "" }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(panel.server_meta(), ("Creative".into(), None));
        let snap = expect_snapshot(capture);

        // External rename causes conflict.
        panel.set_server_meta("Skyblock", Some("New desc"));
        let conflict = execute(panel.clone(), &scope_a(), &grants, &snap, false).await;
        assert!(matches!(conflict, Err(RollbackError::Conflict(_))));

        // Force restores original name and description.
        execute(panel.clone(), &scope_a(), &grants, &snap, true)
            .await
            .unwrap();
        assert_eq!(
            panel.server_meta(),
            ("Survival".into(), Some("Main SMP".into()))
        );
    }

    #[tokio::test]
    async fn create_schedule_rollback_deletes_created_schedule() {
        let panel = StatefulPanel::new();
        let grants = Grants::everything();

        let (outcome, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &grants,
            "create_schedule",
            &json!({ "name": "Nightly", "cron": "0 0 4 * * *" }),
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(panel.schedule_exists("sched-1"));
        let snap = expect_snapshot(capture);

        execute(panel.clone(), &scope_a(), &grants, &snap, false)
            .await
            .unwrap();
        assert!(!panel.schedule_exists("sched-1"));
    }

    #[tokio::test]
    async fn rollback_enforces_permissions_and_server_scope() {
        let panel = StatefulPanel::new();
        let all_grants = Grants::everything();
        panel.set_file("/note.txt", "before");

        let (_, capture) = tools::run_confirmed(
            panel.clone(),
            &scope_a(),
            &all_grants,
            "write_file",
            &json!({ "file": "/note.txt", "content": "after" }),
        )
        .await;
        let snap = expect_snapshot(capture);

        // Missing files.read-content refuses non-forced rollback, but allows forced rollback with files.create.
        let create_only = Grants::server(&["files.create"]);
        let err = execute(panel.clone(), &scope_a(), &create_only, &snap, false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, RollbackError::Failed(ref msg) if msg.contains("files.read-content")),
            "{err:?}"
        );

        // Missing files.create refuses even forced rollback.
        let read_only = Grants::server(&["files.read-content"]);
        let err = execute(panel.clone(), &scope_a(), &read_only, &snap, true)
            .await
            .unwrap_err();
        assert!(
            matches!(err, RollbackError::Failed(ref msg) if msg.contains("files.create")),
            "{err:?}"
        );

        // A snapshot for SERVER_A cannot be rolled back from SERVER_B's scope.
        let scope_b = Scope::Server(SERVER_B.into());
        let err = execute(panel.clone(), &scope_b, &all_grants, &snap, false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, RollbackError::Failed(ref msg) if msg.contains("This chat belongs to server")),
            "{err:?}"
        );

        // Forced rollback with files.create succeeds on SERVER_A.
        execute(panel.clone(), &scope_a(), &create_only, &snap, true)
            .await
            .unwrap();
        assert_eq!(panel.file_text("/note.txt").as_deref(), Some("before"));
    }
}
