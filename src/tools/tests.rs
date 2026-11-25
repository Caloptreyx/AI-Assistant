//! The two gates in front of the panel: tools the user lacks permissions for are neither offered
//! nor run, and changes never reach the panel without the user's confirmation.

use super::*;
use crate::openapi::Index;
use axum::{
    body::Body,
    http::{Method, Request, Response, header},
};
use std::sync::Mutex;

const SERVER: &str = "123e4567-e89b-12d3-a456-426614174000";

/// A panel that records every request and answers all of them with `status`.
struct Panel {
    status: u16,
    calls: Mutex<Vec<(Method, String)>>,
    bodies: Mutex<Vec<Value>>,
}

impl Panel {
    fn new(status: u16) -> Arc<Self> {
        Arc::new(Self {
            status,
            calls: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(Method, String)> {
        self.calls.lock().unwrap().clone()
    }

    /// The JSON bodies of the requests that had one, in order.
    fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Upstream for Panel {
    async fn send(&self, request: Request<Body>) -> Result<Response<Body>, String> {
        let (parts, body) = request.into_parts();
        let path = parts.uri.path().to_string();
        self.calls.lock().unwrap().push((parts.method, path.clone()));
        let sent = axum::body::to_bytes(body, usize::MAX)
            .await
            .map_err(|err| err.to_string())?;
        if let Ok(json) = serde_json::from_slice::<Value>(&sent) {
            self.bodies.lock().unwrap().push(json);
        }
        let body = if self.status >= 400 {
            json!({ "errors": ["you do not have permission to perform this action: files.read"] })
                .to_string()
        } else if path.ends_with("/logs") {
            "[12:00:00] Done (3.1s)!\n[12:00:05] There are 0 of 20 players online".to_string()
        } else {
            json!({ "ok": true, "schedule": { "uuid": "5f0e" }, "server": { "description": "Kept" } })
                .to_string()
        };
        Response::builder()
            .status(self.status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .map_err(|err| err.to_string())
    }

    async fn openapi(&self) -> Result<Arc<Index>, String> {
        Ok(Arc::new(Index::new(&json!({ "paths": {} }))))
    }
}

/// Grants exactly the listed permissions.
struct Grants {
    server: Vec<&'static str>,
    admin: Vec<&'static str>,
}

impl Grants {
    fn server(server: &[&'static str]) -> Self {
        Self {
            server: server.to_vec(),
            admin: Vec::new(),
        }
    }

    fn admin(admin: &[&'static str]) -> Self {
        Self {
            server: Vec::new(),
            admin: admin.to_vec(),
        }
    }

    /// Every permission any tool asks for.
    fn everything() -> Self {
        let mut grants = Self::server(&[]);
        for tool in all() {
            match tool.needs {
                Needs::Nothing => {}
                Needs::Server(list) | Needs::AnyServer(list) => grants.server.extend(list),
                Needs::Admin(list) => grants.admin.extend(list),
            }
        }
        grants
    }
}

impl Permissions for Grants {
    fn server(&self, permission: &str) -> bool {
        self.server.iter().any(|granted| *granted == permission)
    }

    fn admin(&self, permission: &str) -> bool {
        self.admin.iter().any(|granted| *granted == permission)
    }
}

fn server_scope() -> Scope {
    Scope::Server(SERVER.into())
}

fn names(scope: &Scope, grants: &Grants) -> Vec<&'static str> {
    available(scope, grants)
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

/// Arguments that pass each changing tool's own validation, so only the gates can stop it.
fn sample_arguments(name: &str) -> Value {
    let arguments = match name {
        "power_server" => json!({ "action": "restart" }),
        "send_command" | "run_command" => json!({ "command": "list", "timeout_seconds": 1 }),
        "rename_server" => json!({ "name": "Survival" }),
        "write_file" => json!({ "file": "/eula.txt", "content": "eula=true" }),
        "edit_file" => json!({ "file": "/x.json", "old_string": "ok", "new_string": "no" }),
        "delete_files" => json!({ "files": ["old.log"] }),
        "create_backup" => json!({ "name": "before update" }),
        "restore_backup" | "delete_backup" => json!({ "backup": "5f0e" }),
        "create_schedule" => json!({ "name": "nightly", "cron": "0 0 4 * * *" }),
        "run_schedule" => json!({ "schedule": "5f0e" }),
        "update_startup_variables" => json!({ "variables": { "SERVER_JARFILE": "paper.jar" } }),
        "write_api" => json!({ "method": "POST", "path": "/api/admin/users", "body": {} }),
        other => panic!("add sample arguments for the changing tool {other}"),
    };
    let mut arguments = arguments;
    arguments["server"] = json!(SERVER);
    arguments
}

#[test]
fn every_tool_is_declared_once_with_a_valid_schema() {
    let mut seen = std::collections::HashSet::new();
    for tool in all() {
        assert!(seen.insert(tool.name), "{} is declared twice", tool.name);
        assert!(find(tool.name).is_some());
        for scope in [server_scope(), Scope::Admin] {
            let schema = tool.schema(&scope);
            let properties = schema["properties"].as_object().unwrap();
            for required in schema["required"].as_array().unwrap() {
                assert!(
                    properties.contains_key(required.as_str().unwrap()),
                    "{} requires an undeclared property {required}",
                    tool.name
                );
            }
        }
    }
}

#[test]
fn a_server_chat_offers_only_what_the_subuser_may_do() {
    let grants = Grants::server(&["control.read-console", "files.read"]);
    assert_eq!(
        names(&server_scope(), &grants),
        ["get_server", "read_console", "list_files"]
    );

    // Power needs any one of start, stop and restart; the panel checks the action itself.
    let grants = Grants::server(&["control.start"]);
    assert!(names(&server_scope(), &grants).contains(&"power_server"));

    let grants = Grants::server(&["files.read-content"]);
    let offered = names(&server_scope(), &grants);
    assert!(offered.contains(&"read_file"));
    assert!(!offered.contains(&"edit_file"), "edit_file also needs files.create");
}

#[test]
fn a_server_chat_never_offers_admin_or_raw_api_tools() {
    let offered = available(&server_scope(), &Grants::everything());
    assert!(!offered.is_empty());
    for tool in offered {
        assert!(tool.server_arg && !tool.admin_only, "{} is offered", tool.name);
    }
}

#[test]
fn the_admin_chat_follows_admin_permissions() {
    let offered = names(&Scope::Admin, &Grants::admin(&["nodes.read"]));
    for name in ["list_nodes", "get_node_overview", "read_api", "write_api", "get_server"] {
        assert!(offered.contains(&name), "{name} is missing");
    }
    for name in ["list_users", "panel_stats", "list_all_servers"] {
        assert!(!offered.contains(&name), "{name} is offered without its permission");
    }
}

#[test]
fn the_server_argument_exists_only_in_the_admin_chat() {
    let tool = find("read_console").unwrap();
    let server_schema = tool.schema(&server_scope());
    assert!(server_schema["properties"].get("server").is_none());
    let admin_schema = tool.schema(&Scope::Admin);
    assert!(admin_schema["properties"].get("server").is_some());
    assert_eq!(admin_schema["required"][0], json!("server"));

    let change = find("power_server").unwrap().spec(&server_scope());
    assert!(change.description.contains("confirm"));
}

#[tokio::test]
async fn tools_without_permission_are_refused_before_any_request() {
    let panel = Panel::new(200);
    let nothing = Grants::server(&[]);

    let outcome = run_unconfirmed(
        panel.clone(),
        &server_scope(),
        &nothing,
        "read_console",
        &json!({}),
    )
    .await;
    match outcome {
        Unconfirmed::Ran(Err(err)) => assert!(err.contains("not available"), "{err}"),
        _ => panic!("read_console ran without control.read-console"),
    }

    let (outcome, capture) = run_confirmed(
        panel.clone(),
        &server_scope(),
        &nothing,
        "power_server",
        &sample_arguments("power_server"),
    )
    .await;
    let err = outcome.unwrap_err();
    assert!(err.contains("not available"), "{err}");
    assert_eq!(capture, RollbackCapture::none());

    // Admin tools do not exist in a server chat, whatever the user may do elsewhere.
    let admin = Grants::admin(&["users.read"]);
    assert!(matches!(
        run_unconfirmed(panel.clone(), &server_scope(), &admin, "list_users", &json!({})).await,
        Unconfirmed::Ran(Err(_))
    ));
    assert!(matches!(
        run_unconfirmed(panel.clone(), &Scope::Admin, &admin, "rm_rf", &json!({})).await,
        Unconfirmed::Ran(Err(_))
    ));

    assert_eq!(panel.calls(), Vec::new());
}

#[tokio::test]
async fn changes_wait_for_confirmation_without_sending_anything() {
    let grants = Grants::everything();
    for scope in [server_scope(), Scope::Admin] {
        for tool in available(&scope, &grants) {
            if tool.effect != Effect::Change {
                continue;
            }
            let panel = Panel::new(200);
            let outcome = run_unconfirmed(
                panel.clone(),
                &scope,
                &grants,
                tool.name,
                &sample_arguments(tool.name),
            )
            .await;
            assert!(
                matches!(outcome, Unconfirmed::NeedsConfirmation(proposed) if proposed.name == tool.name),
                "{} ran without confirmation",
                tool.name
            );
            assert_eq!(panel.calls(), Vec::new(), "{} reached the panel", tool.name);
        }
    }
}

#[tokio::test]
async fn an_unconfirmed_run_cannot_send_anything_but_get() {
    // Even if a changing tool were run on the unconfirmed path, the access check stops every
    // request that is not a GET before it reaches the panel.
    for tool in all().iter().filter(|tool| tool.effect == Effect::Change) {
        let panel = Panel::new(200);
        let Value::Object(args) = sample_arguments(tool.name) else {
            unreachable!()
        };
        let result = execute(&ctx(panel.clone(), &Scope::Admin, false), tool, &args).await;
        assert!(result.is_err(), "{} succeeded unconfirmed", tool.name);
        for (method, path) in panel.calls() {
            assert_eq!(method, Method::GET, "{} sent {method} {path}", tool.name);
        }
    }
}

#[tokio::test]
async fn renaming_keeps_the_current_description() {
    // The panel's rename endpoint requires `description`; leaving it out failed with HTTP 400
    // on the dev panel, and sending null would erase it.
    let panel = Panel::new(200);
    let grants = Grants::server(&["settings.rename"]);
    let (result, _) = run_confirmed(
        panel.clone(),
        &server_scope(),
        &grants,
        "rename_server",
        &json!({ "name": "Survival Two" }),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        panel.calls(),
        vec![
            (Method::GET, format!("/api/client/servers/{SERVER}")),
            (Method::POST, format!("/api/client/servers/{SERVER}/settings/rename")),
        ]
    );
    assert_eq!(
        panel.bodies(),
        vec![json!({ "name": "Survival Two", "description": "Kept" })]
    );

    // An empty description removes it.
    let panel = Panel::new(200);
    run_confirmed(
        panel.clone(),
        &server_scope(),
        &grants,
        "rename_server",
        &json!({ "description": "" }),
    )
    .await
    .0
    .unwrap();
    assert_eq!(panel.bodies(), vec![json!({ "description": null })]);
}

#[tokio::test]
async fn confirmed_changes_reach_the_panel() {
    let panel = Panel::new(200);
    let (result, capture) = run_confirmed(
        panel.clone(),
        &server_scope(),
        &Grants::server(&["control.restart"]),
        "power_server",
        &sample_arguments("power_server"),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(capture, RollbackCapture::none());
    assert_eq!(
        panel.calls(),
        vec![(Method::POST, format!("/api/client/servers/{SERVER}/power"))]
    );
}

#[tokio::test]
async fn a_server_chat_only_ever_reaches_its_own_server() {
    let panel = Panel::new(200);
    let grants = Grants::everything();

    let outcome = run_unconfirmed(
        panel.clone(),
        &server_scope(),
        &grants,
        "read_console",
        &json!({ "server": "0a1b2c3d", "lines": 5 }),
    )
    .await;
    match outcome {
        Unconfirmed::Ran(Ok(text)) => assert!(text.contains("players online"), "{text}"),
        _ => panic!("read_console failed"),
    }
    let (result, _) = run_confirmed(
        panel.clone(),
        &server_scope(),
        &grants,
        "send_command",
        &json!({ "server": "0a1b2c3d", "command": "say hi" }),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");

    assert_eq!(
        panel.calls(),
        vec![
            (Method::GET, format!("/api/client/servers/{SERVER}/logs")),
            (Method::POST, format!("/api/client/servers/{SERVER}/command")),
        ]
    );
}

#[tokio::test]
async fn panel_permission_errors_come_back_to_the_model() {
    let panel = Panel::new(403);
    let outcome = run_unconfirmed(
        panel.clone(),
        &server_scope(),
        &Grants::server(&["files.read"]),
        "list_files",
        &json!({}),
    )
    .await;
    match outcome {
        Unconfirmed::Ran(Err(err)) => {
            assert!(err.contains("HTTP 403"), "{err}");
            assert!(err.contains("permission"), "{err}");
        }
        _ => panic!("a refused request did not come back as an error"),
    }
}

#[tokio::test]
async fn read_api_cannot_reach_credentials_or_the_assistant_itself() {
    let panel = Panel::new(200);
    for path in [
        "/api/client/servers/0a1b2c3d/websocket",
        "/api/admin/nodes/5f0e/token",
        "/api/admin/extensions/dev.caloptreyx.aiassistant/settings",
        "/api/client/servers/0a1b2c3d/ai-assistant/conversations",
    ] {
        let outcome = run_unconfirmed(
            panel.clone(),
            &Scope::Admin,
            &Grants::admin(&[]),
            "read_api",
            &json!({ "path": path }),
        )
        .await;
        assert!(matches!(outcome, Unconfirmed::Ran(Err(_))), "{path} was read");
    }
    assert_eq!(panel.calls(), Vec::new());
}

#[tokio::test]
async fn arguments_that_are_not_an_object_are_refused() {
    let panel = Panel::new(200);
    let outcome = run_unconfirmed(
        panel.clone(),
        &Scope::Admin,
        &Grants::admin(&[]),
        "search_api",
        &json!("{\"query\":"),
    )
    .await;
    match outcome {
        Unconfirmed::Ran(Err(err)) => assert!(err.contains("not valid JSON"), "{err}"),
        _ => panic!("broken arguments were accepted"),
    }
    assert_eq!(panel.calls(), Vec::new());
}
