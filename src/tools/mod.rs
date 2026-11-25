//! The tools the model can call. Each one is a sequence of requests to the panel's own API,
//! sent through the panel's router as the chatting user (see [`crate::upstream`]), so the panel's
//! permission checks decide every request exactly as if the user clicked through it.
//!
//! Two gates sit in front of that:
//! - [`Tool::allowed`] hides tools the user lacks the permissions for, so the model is not even
//!   offered them, and refuses them again when they are called anyway.
//! - [`run_unconfirmed`] runs read tools only. Tools that change something come back as
//!   [`Unconfirmed::NeedsConfirmation`] without sending a single request; they run through
//!   [`run_confirmed`] once the user clicked Confirm. Unconfirmed runs also use an [`Access`]
//!   that lets only GET requests through, so a misclassified tool still cannot change anything.

mod admin;
mod api;
pub mod args;
mod definitions;
mod files;
mod manage;
pub mod output;
pub mod rollback;
mod servers;
#[cfg(test)]
mod tests;

use crate::{access::Access, provider::ToolSpec, upstream::Upstream};
use rollback::RollbackCapture;
use serde_json::{Map, Value, json};
use shared::models::user::PermissionManager;
use std::sync::Arc;

pub use definitions::all;

pub struct Ctx {
    pub upstream: Arc<dyn Upstream>,
    pub access: Access,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Runs as soon as the model asks.
    Read,
    /// Shown to the user as an action card; runs only after they confirm.
    Change,
}

/// The permissions a tool needs before it is offered. The panel checks every request again.
#[derive(Clone, Copy, Debug)]
pub enum Needs {
    Nothing,
    /// All of these server permissions (checked in server chats; the admin chat leaves them to
    /// the panel, server by server).
    Server(&'static [&'static str]),
    /// At least one of these server permissions.
    AnyServer(&'static [&'static str]),
    /// All of these admin permissions.
    Admin(&'static [&'static str]),
}

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub effect: Effect,
    /// Only in the admin chat.
    pub admin_only: bool,
    /// Works on one server: bound to the chat's server in server chats, a `server` argument in
    /// the admin chat.
    pub server_arg: bool,
    pub needs: Needs,
    properties: Value,
    required: &'static [&'static str],
}

/// Where a chat runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// A server's chat, with the server's UUID.
    Server(String),
    /// The admin area's chat.
    Admin,
}

impl Scope {
    fn server(&self) -> Option<String> {
        match self {
            Self::Server(uuid) => Some(uuid.clone()),
            Self::Admin => None,
        }
    }
}

pub trait Permissions: Send + Sync {
    fn server(&self, permission: &str) -> bool;
    fn admin(&self, permission: &str) -> bool;
}

impl Permissions for PermissionManager {
    fn server(&self, permission: &str) -> bool {
        self.has_server_permission(permission).is_ok()
    }

    fn admin(&self, permission: &str) -> bool {
        self.has_admin_permission(permission).is_ok()
    }
}

impl Tool {
    pub fn allowed(&self, scope: &Scope, permissions: &dyn Permissions) -> bool {
        let in_scope = match scope {
            Scope::Server(_) => self.server_arg && !self.admin_only,
            Scope::Admin => true,
        };
        in_scope
            && match (self.needs, scope) {
                (Needs::Nothing, _) => true,
                (Needs::Server(_) | Needs::AnyServer(_), Scope::Admin) => true,
                (Needs::Server(all), Scope::Server(_)) => {
                    all.iter().all(|permission| permissions.server(permission))
                }
                (Needs::AnyServer(any), Scope::Server(_)) => {
                    any.iter().any(|permission| permissions.server(permission))
                }
                (Needs::Admin(all), _) => all.iter().all(|permission| permissions.admin(permission)),
            }
    }

    pub fn schema(&self, scope: &Scope) -> Value {
        let mut properties = match &self.properties {
            Value::Object(properties) => properties.clone(),
            _ => Map::new(),
        };
        let mut required: Vec<&str> = self.required.to_vec();
        if self.server_arg && *scope == Scope::Admin {
            properties.insert(
                "server".into(),
                json!({ "type": "string", "description": "Server UUID or short ID, as returned by list_all_servers or list_servers" }),
            );
            required.insert(0, "server");
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    }

    pub fn spec(&self, scope: &Scope) -> ToolSpec {
        let description = match self.effect {
            Effect::Read => self.description.to_string(),
            Effect::Change => format!(
                "{} This changes something: the user sees the call as an action card and it only runs after they confirm it.",
                self.description
            ),
        };
        ToolSpec {
            name: self.name.to_string(),
            description,
            schema: self.schema(scope),
        }
    }
}

pub fn find(name: &str) -> Option<&'static Tool> {
    all().iter().find(|tool| tool.name == name)
}

/// The tools offered to the model in this chat, in a stable order.
pub fn available(scope: &Scope, permissions: &dyn Permissions) -> Vec<&'static Tool> {
    all()
        .iter()
        .filter(|tool| tool.allowed(scope, permissions))
        .collect()
}

/// A tool the model may call here, with its arguments ready to run: in a server chat the
/// `server` argument is always the chat's own server, whatever the model sent.
fn prepare(
    scope: &Scope,
    permissions: &dyn Permissions,
    name: &str,
    arguments: &Value,
) -> Result<(&'static Tool, Map<String, Value>), String> {
    let tool = find(name)
        .filter(|tool| tool.allowed(scope, permissions))
        .ok_or_else(|| {
            format!(
                "The tool {name} is not available in this chat: it does not exist or the user lacks the permission for it."
            )
        })?;
    let mut args = match arguments {
        Value::Object(args) => args.clone(),
        Value::String(_) => {
            return Err("The tool arguments were not valid JSON. Send them again as a JSON object.".into());
        }
        _ => return Err("The tool arguments must be a JSON object.".into()),
    };
    if let Scope::Server(uuid) = scope {
        if tool.server_arg {
            args.insert("server".into(), json!(uuid));
        }
    }
    Ok((tool, args))
}

pub(super) fn ctx(upstream: Arc<dyn Upstream>, scope: &Scope, confirmed: bool) -> Ctx {
    Ctx {
        upstream,
        access: Access {
            confirmed,
            server: scope.server(),
        },
    }
}

pub enum Unconfirmed {
    /// A read tool ran (or the call was refused); this is the result for the model.
    Ran(Result<String, String>),
    /// A change: nothing was sent; the user has to confirm it first.
    NeedsConfirmation(&'static Tool),
}

/// Runs a tool call straight from the model.
pub async fn run_unconfirmed(
    upstream: Arc<dyn Upstream>,
    scope: &Scope,
    permissions: &dyn Permissions,
    name: &str,
    arguments: &Value,
) -> Unconfirmed {
    let (tool, args) = match prepare(scope, permissions, name, arguments) {
        Ok(prepared) => prepared,
        Err(err) => return Unconfirmed::Ran(Err(err)),
    };
    match tool.effect {
        Effect::Change => Unconfirmed::NeedsConfirmation(tool),
        Effect::Read => Unconfirmed::Ran(
            execute(&ctx(upstream, scope, false), tool, &args)
                .await
                .map(|(text, _)| text),
        ),
    }
}

/// Runs a call the user confirmed. Only the confirm endpoint calls this, after it atomically
/// moved the stored action from pending to running. Permissions are checked again because they
/// may have changed since the model proposed the call.
///
/// Returns `(outcome, rollback_capture)`. Whenever `outcome` is `Err`, `rollback_capture` is
/// always [`RollbackCapture::Unavailable`] with no snapshot.
pub async fn run_confirmed(
    upstream: Arc<dyn Upstream>,
    scope: &Scope,
    permissions: &dyn Permissions,
    name: &str,
    arguments: &Value,
) -> (Result<String, String>, RollbackCapture) {
    let (tool, args) = match prepare(scope, permissions, name, arguments) {
        Ok(prepared) => prepared,
        Err(err) => return (Err(err), RollbackCapture::none()),
    };
    match execute(&ctx(upstream, scope, true), tool, &args).await {
        Ok((text, capture)) => (Ok(text), capture),
        Err(err) => (Err(err), RollbackCapture::none()),
    }
}

async fn execute(
    ctx: &Ctx,
    tool: &Tool,
    args: &Map<String, Value>,
) -> Result<(String, RollbackCapture), String> {
    let plain = |result: Result<String, String>| result.map(|text| (text, RollbackCapture::none()));
    match tool.name {
        "get_server" => plain(servers::get(ctx, args).await),
        "list_servers" => plain(servers::list(ctx, args).await),
        "read_console" => plain(servers::console(ctx, args).await),
        "power_server" => plain(servers::power(ctx, args).await),
        "send_command" => plain(servers::command(ctx, args).await),
        "run_command" => plain(servers::run_command(ctx, args).await),
        "rename_server" => servers::rename(ctx, args).await,
        "list_files" => plain(files::list(ctx, args).await),
        "read_file" => plain(files::read(ctx, args).await),
        "write_file" => files::write(ctx, args).await,
        "edit_file" => files::edit(ctx, args).await,
        "delete_files" => files::delete(ctx, args).await,
        "list_backups" => plain(manage::list_backups(ctx, args).await),
        "create_backup" => plain(manage::create_backup(ctx, args).await),
        "restore_backup" => plain(manage::restore_backup(ctx, args).await),
        "delete_backup" => plain(manage::delete_backup(ctx, args).await),
        "list_schedules" => plain(manage::list_schedules(ctx, args).await),
        "create_schedule" => manage::create_schedule(ctx, args).await,
        "run_schedule" => plain(manage::run_schedule(ctx, args).await),
        "list_startup_variables" => plain(manage::list_variables(ctx, args).await),
        "update_startup_variables" => manage::update_variables(ctx, args).await,
        "panel_stats" => plain(admin::stats(ctx).await),
        "list_nodes" => plain(admin::list_nodes(ctx, args).await),
        "get_node_overview" => plain(admin::node_overview(ctx, args).await),
        "list_users" => plain(admin::list_users(ctx, args).await),
        "list_all_servers" => plain(admin::list_all_servers(ctx, args).await),
        "search_api" => plain(api::search(ctx, args).await),
        "describe_api" => plain(api::describe(ctx, args).await),
        "read_api" => plain(api::read(ctx, args).await),
        "write_api" => plain(api::write(ctx, args).await),
        other => Err(format!("Unknown tool: {other}")),
    }
}
