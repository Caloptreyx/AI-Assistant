//! Every tool the assistant knows. Most descriptions and schemas follow Cal MCP's tools.

use super::{Effect, Needs, Tool};
use serde_json::{Value, json};
use std::sync::LazyLock;

#[allow(clippy::too_many_arguments)]
fn tool(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    effect: Effect,
    needs: Needs,
    server_arg: bool,
    admin_only: bool,
    properties: Value,
    required: &'static [&'static str],
) -> Tool {
    Tool {
        name,
        title,
        description,
        effect,
        admin_only,
        server_arg,
        needs,
        properties,
        required,
    }
}

/// A tool about one server: available in server chats (bound to that server) and in the admin
/// chat (with a `server` argument).
fn server_tool(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    effect: Effect,
    needs: Needs,
    properties: Value,
    required: &'static [&'static str],
) -> Tool {
    tool(
        name,
        title,
        description,
        effect,
        needs,
        true,
        false,
        properties,
        required,
    )
}

/// A tool that only exists in the admin chat.
fn admin_tool(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    effect: Effect,
    needs: Needs,
    properties: Value,
    required: &'static [&'static str],
) -> Tool {
    tool(
        name,
        title,
        description,
        effect,
        needs,
        false,
        true,
        properties,
        required,
    )
}

fn server_tools() -> Vec<Tool> {
    let command = json!({ "type": "string", "minLength": 1, "maxLength": 1024 });
    let expected = json!({
        "type": "string",
        "description": "The sha256 read_file reported. The change is refused if the file has changed since.",
    });
    let page = json!({ "type": "integer", "minimum": 1, "default": 1 });
    let per_page = json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 });
    let backup = json!({ "type": "string", "description": "Backup UUID, as returned by list_backups" });
    let schedule =
        json!({ "type": "string", "description": "Schedule UUID, as returned by list_schedules" });

    vec![
        server_tool(
            "get_server",
            "Get server details",
            "Show the server's configuration (name, egg, limits, allocation) together with its live state and resource usage (CPU, memory, disk, network, uptime).",
            Effect::Read,
            Needs::Nothing,
            json!({}),
            &[],
        ),
        server_tool(
            "read_console",
            "Read the console",
            "Return the most recent console output of the server.",
            Effect::Read,
            Needs::Server(&["control.read-console"]),
            json!({
                "lines": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 100, "description": "How many of the last lines to return" },
            }),
            &[],
        ),
        server_tool(
            "power_server",
            "Start, stop, restart or kill the server",
            "Send a power action. kill stops the process immediately without a clean shutdown. \
With wait, the tool waits until the server is running (start, restart) or offline (stop, kill), and reports a crash during start-up.",
            Effect::Change,
            Needs::AnyServer(&["control.start", "control.stop", "control.restart"]),
            json!({
                "action": { "type": "string", "enum": ["start", "stop", "restart", "kill"] },
                "wait": { "type": "boolean", "default": false, "description": "Wait until the server reaches the new state" },
                "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120, "default": 60 },
            }),
            &["action"],
        ),
        server_tool(
            "send_command",
            "Send a console command",
            "Run a command in the running server's console, for example \"say hello\", without waiting for its output. \
To see what the command prints, use run_command instead.",
            Effect::Change,
            Needs::Server(&["control.console"]),
            json!({ "command": command }),
            &["command"],
        ),
        server_tool(
            "run_command",
            "Run a console command and wait for its output",
            "Run a command in the running server's console and return only the output it produced. \
With wait_for, waits until that text (or regular expression, with regex) appears; without it, returns once the console is quiet.",
            Effect::Change,
            Needs::Server(&["control.console", "control.read-console"]),
            json!({
                "command": command,
                "wait_for": { "type": "string", "description": "Text to wait for in the new output, for example \"Done\"" },
                "regex": { "type": "boolean", "default": false, "description": "Treat wait_for as a regular expression" },
                "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 60, "default": 10 },
            }),
            &["command"],
        ),
        server_tool(
            "rename_server",
            "Rename the server",
            "Change the server's name and/or description. An empty description removes it.",
            Effect::Change,
            Needs::Server(&["settings.rename"]),
            json!({
                "name": { "type": "string", "maxLength": 255 },
                "description": { "type": "string", "maxLength": 1024 },
            }),
            &[],
        ),
        server_tool(
            "list_files",
            "List files",
            "List the files and folders in a directory of the server.",
            Effect::Read,
            Needs::Server(&["files.read"]),
            json!({
                "directory": { "type": "string", "default": "/", "description": "Directory to list, for example /plugins" },
                "page": page,
                "per_page": { "type": "integer", "minimum": 1, "maximum": 100, "default": 100 },
            }),
            &[],
        ),
        server_tool(
            "read_file",
            "Read a file",
            "Return the contents of a text file such as server.properties or a plugin config, followed by a note with its size, line count and sha256. \
Large files come back in parts: read on with start_line/end_line (one-based, inclusive) or offset/length (bytes). \
Never pass a part to write_file; change large files with edit_file.",
            Effect::Read,
            Needs::Server(&["files.read-content"]),
            json!({
                "file": { "type": "string", "description": "File path, for example /server.properties" },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "offset": { "type": "integer", "minimum": 0 },
                "length": { "type": "integer", "minimum": 1 },
            }),
            &["file"],
        ),
        server_tool(
            "write_file",
            "Write a file",
            "Create a text file or replace its whole contents. To change part of an existing file, use edit_file. \
Files over 100 KB are only replaced when expected_sha256 is given.",
            Effect::Change,
            Needs::Server(&["files.create"]),
            json!({
                "file": { "type": "string", "description": "File path, for example /config/settings.yml" },
                "content": { "type": "string", "description": "The complete new contents of the file" },
                "expected_sha256": expected,
            }),
            &["file", "content"],
        ),
        server_tool(
            "edit_file",
            "Edit a file",
            "Replace exact text in a text file, leaving the rest untouched. \
old_string must match the file exactly, including indentation and line breaks, and must occur once unless replace_all is set.",
            Effect::Change,
            Needs::Server(&["files.read-content", "files.create"]),
            json!({
                "file": { "type": "string" },
                "old_string": { "type": "string", "minLength": 1, "description": "The exact text to replace" },
                "new_string": { "type": "string", "description": "The text to put in its place" },
                "replace_all": { "type": "boolean", "default": false },
                "expected_sha256": expected,
            }),
            &["file", "old_string", "new_string"],
        ),
        server_tool(
            "delete_files",
            "Delete files",
            "Delete files or directories (with everything in them) inside one directory.",
            Effect::Change,
            Needs::Server(&["files.delete"]),
            json!({
                "root": { "type": "string", "default": "/", "description": "The directory the names are in" },
                "files": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 100, "description": "File or directory names inside root" },
            }),
            &["files"],
        ),
        server_tool(
            "list_backups",
            "List backups",
            "List the server's backups with their UUID, name, size, whether they completed and whether they are locked.",
            Effect::Read,
            Needs::Server(&["backups.read"]),
            json!({ "search": { "type": "string" }, "page": page, "per_page": per_page }),
            &[],
        ),
        server_tool(
            "create_backup",
            "Create a backup",
            "Start a backup of the server's files. It runs in the background; list_backups shows when it has completed.",
            Effect::Change,
            Needs::Server(&["backups.create"]),
            json!({
                "name": { "type": "string", "maxLength": 255, "description": "Defaults to the current date and time" },
                "ignored_files": { "type": "array", "items": { "type": "string" }, "description": "Paths or patterns to leave out, like .gitignore lines" },
            }),
            &[],
        ),
        server_tool(
            "restore_backup",
            "Restore a backup",
            "Replace the server's files with a backup. Files changed since the backup are overwritten; with truncate_directory, every file not in the backup is deleted first.",
            Effect::Change,
            Needs::Server(&["backups.restore"]),
            json!({
                "backup": backup,
                "truncate_directory": { "type": "boolean", "default": false },
                "restore_startup": { "type": "boolean", "default": false, "description": "Also restore the startup command and variables saved with the backup" },
            }),
            &["backup"],
        ),
        server_tool(
            "delete_backup",
            "Delete a backup",
            "Delete a backup permanently.",
            Effect::Change,
            Needs::Server(&["backups.delete"]),
            json!({ "backup": backup }),
            &["backup"],
        ),
        server_tool(
            "list_schedules",
            "List schedules",
            "List the server's schedules with their UUID, triggers, condition and when they last ran.",
            Effect::Read,
            Needs::Server(&["schedules.read"]),
            json!({ "search": { "type": "string" }, "page": page, "per_page": per_page }),
            &[],
        ),
        server_tool(
            "create_schedule",
            "Create a schedule",
            "Create a schedule that runs steps on the server. cron adds a time trigger. \
Each step is an action object, for example {\"type\": \"send_command\", \"command\": \"say Restarting\", \"ignore_failure\": false}, \
{\"type\": \"sleep\", \"duration\": 5000}, {\"type\": \"send_power\", \"action\": \"restart\", \"ignore_failure\": false} or \
{\"type\": \"create_backup\", \"foreground\": true, \"ignored_files\": [], \"ignore_failure\": false}.",
            Effect::Change,
            Needs::Server(&["schedules.create"]),
            json!({
                "name": { "type": "string", "minLength": 1, "maxLength": 255 },
                "cron": { "type": "string", "description": "Six fields: second minute hour day month weekday, for example \"0 0 4 * * *\" for 04:00 every day" },
                "triggers": { "type": "array", "items": { "type": "object" } },
                "condition": { "type": "object" },
                "steps": { "type": "array", "items": { "type": "object" }, "maxItems": 100 },
                "enabled": { "type": "boolean", "default": true },
            }),
            &["name"],
        ),
        server_tool(
            "run_schedule",
            "Run a schedule now",
            "Run a schedule's steps now, in the background.",
            Effect::Change,
            Needs::Server(&["schedules.update"]),
            json!({
                "schedule": schedule,
                "skip_condition": { "type": "boolean", "default": false },
            }),
            &["schedule"],
        ),
        server_tool(
            "list_startup_variables",
            "List startup variables",
            "List the server's startup variables with their name, environment variable, current value, rules and whether they may be edited.",
            Effect::Read,
            Needs::Server(&["startup.read"]),
            json!({}),
            &[],
        ),
        server_tool(
            "update_startup_variables",
            "Update startup variables",
            "Set new values for startup variables. Only the variables named change; the server needs a restart to use them.",
            Effect::Change,
            Needs::Server(&["startup.update"]),
            json!({
                "variables": {
                    "type": "object",
                    "minProperties": 1,
                    "additionalProperties": { "type": ["string", "number", "boolean"] },
                    "description": "Environment variable names and their new values, for example {\"MINECRAFT_VERSION\": \"1.21.4\"}",
                },
            }),
            &["variables"],
        ),
    ]
}

fn admin_tools() -> Vec<Tool> {
    let page = json!({ "type": "integer", "minimum": 1, "default": 1 });
    let per_page = json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 });
    let api_path = json!({
        "type": "string",
        "description": "Endpoint path starting with /api/client/ or /api/admin/, every {placeholder} replaced with a real value",
    });
    let query = json!({
        "type": "object",
        "description": "Query parameters, for example {\"page\": 1, \"per_page\": 25}",
        "additionalProperties": { "type": ["string", "number", "boolean", "array", "null"] },
    });

    vec![
        admin_tool(
            "panel_stats",
            "Panel statistics",
            "Count the panel's users, servers, nodes, locations, eggs, database hosts, backup configurations and roles.",
            Effect::Read,
            Needs::Admin(&["stats.read"]),
            json!({}),
            &[],
        ),
        admin_tool(
            "list_nodes",
            "List nodes",
            "List the panel's nodes with their UUID, name, location, URL and limits.",
            Effect::Read,
            Needs::Admin(&["nodes.read"]),
            json!({ "search": { "type": "string" }, "page": page, "per_page": per_page }),
            &[],
        ),
        admin_tool(
            "get_node_overview",
            "Node system overview",
            "Show a node's system overview from Wings: version, CPU, memory, disk and container counts.",
            Effect::Read,
            Needs::Admin(&["nodes.read"]),
            json!({ "node": { "type": "string", "description": "Node UUID, as returned by list_nodes" } }),
            &["node"],
        ),
        admin_tool(
            "list_users",
            "List users",
            "List the panel's users with their UUID, username, email, role and whether they are admins or suspended.",
            Effect::Read,
            Needs::Admin(&["users.read"]),
            json!({ "search": { "type": "string" }, "page": page, "per_page": per_page }),
            &[],
        ),
        admin_tool(
            "list_all_servers",
            "List all servers",
            "List every server on the panel with its UUID, owner, node, egg, limits and status.",
            Effect::Read,
            Needs::Admin(&["servers.read"]),
            json!({ "search": { "type": "string" }, "page": page, "per_page": per_page }),
            &[],
        ),
        admin_tool(
            "list_servers",
            "List my servers",
            "List the servers this account can open in the client area. With other, admins also see servers owned by other users.",
            Effect::Read,
            Needs::Nothing,
            json!({
                "search": { "type": "string" },
                "page": page,
                "per_page": per_page,
                "other": { "type": "boolean", "default": false },
            }),
            &[],
        ),
        admin_tool(
            "search_api",
            "Search the panel API",
            "Find panel API endpoints by keywords such as \"server backups\", \"create user\" or \"node allocations\". \
Returns method, path and inputs of the best matches (* marks required query parameters). \
Follow up with describe_api for the exact body, then call the endpoint with read_api or write_api.",
            Effect::Read,
            Needs::Nothing,
            json!({
                "query": { "type": "string", "minLength": 1 },
                "scope": { "type": "string", "enum": ["all", "client", "admin"], "default": "all" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 15 },
            }),
            &["query"],
        ),
        admin_tool(
            "describe_api",
            "Describe a panel API endpoint",
            "Show every parameter, the request body schema and the response schema of one endpoint.",
            Effect::Read,
            Needs::Nothing,
            json!({
                "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                "path": { "type": "string", "description": "For example /api/admin/users/{user}" },
            }),
            &["method", "path"],
        ),
        admin_tool(
            "read_api",
            "Read from the panel API",
            "Send a GET request to any /api/client or /api/admin endpoint and return the response.",
            Effect::Read,
            Needs::Nothing,
            json!({ "path": api_path, "query": query }),
            &["path"],
        ),
        admin_tool(
            "write_api",
            "Change data through the panel API",
            "Send a POST, PUT, PATCH or DELETE request to any /api/client or /api/admin endpoint. \
Check the endpoint with describe_api first. `body` is sent as JSON; a plain string is sent as raw text.",
            Effect::Change,
            Needs::Nothing,
            json!({
                "method": { "type": "string", "enum": ["POST", "PUT", "PATCH", "DELETE"] },
                "path": api_path,
                "query": query,
                "body": { "description": "Request body: any JSON value, or a string for raw text" },
            }),
            &["method", "path"],
        ),
    ]
}

static TOOLS: LazyLock<Vec<Tool>> =
    LazyLock::new(|| server_tools().into_iter().chain(admin_tools()).collect());

pub fn all() -> &'static [Tool] {
    &TOOLS
}
