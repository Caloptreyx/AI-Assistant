//! Panel-wide overviews for the admin chat. The panel checks the admin permission of every
//! request (`stats.read`, `nodes.read`, `users.read`, `servers.read`).

use super::{
    Ctx,
    args::{identifier, integer, optional_text, to_map},
    output::request,
};
use axum::http::Method;
use serde_json::{Map, Value, json};

fn page_query(args: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut query = to_map(json!({
        "page": integer(args, "page", 1, 1, i64::MAX)?,
        "per_page": integer(args, "per_page", 25, 1, 100)?,
    }));
    if let Some(search) = optional_text(args, "search")?.filter(|search| !search.is_empty()) {
        query.insert("search".into(), json!(search));
    }
    Ok(query)
}

pub async fn stats(ctx: &Ctx) -> Result<String, String> {
    request(ctx, Method::GET, "/api/admin/stats/general", None, None).await
}

pub async fn list_nodes(ctx: &Ctx, args: &Map<String, Value>) -> Result<String, String> {
    let query = page_query(args)?;
    request(ctx, Method::GET, "/api/admin/nodes", Some(&query), None).await
}

pub async fn node_overview(ctx: &Ctx, args: &Map<String, Value>) -> Result<String, String> {
    let node = identifier(args, "node")?;
    request(
        ctx,
        Method::GET,
        &format!("/api/admin/nodes/{node}/system/overview"),
        None,
        None,
    )
    .await
}

pub async fn list_users(ctx: &Ctx, args: &Map<String, Value>) -> Result<String, String> {
    let query = page_query(args)?;
    request(ctx, Method::GET, "/api/admin/users", Some(&query), None).await
}

pub async fn list_all_servers(ctx: &Ctx, args: &Map<String, Value>) -> Result<String, String> {
    let query = page_query(args)?;
    request(ctx, Method::GET, "/api/admin/servers", Some(&query), None).await
}
