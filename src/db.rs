//! Conversations, their entries, proposed actions and per-user usage.

use crate::{
    chat::entry::{Entry, ToolCall},
    keys::KeySource,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use shared::database::Database;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct Conversation {
    pub uuid: Uuid,
    pub title: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

type ConversationRow = (Uuid, String, DateTime<Utc>, DateTime<Utc>);

impl From<ConversationRow> for Conversation {
    fn from((uuid, title, created, updated): ConversationRow) -> Self {
        Self {
            uuid,
            title,
            created,
            updated,
        }
    }
}

/// `server` is `None` for the admin area's conversations.
pub async fn conversations(
    db: &Database,
    user: Uuid,
    server: Option<Uuid>,
) -> Result<Vec<Conversation>, sqlx::Error> {
    let rows: Vec<ConversationRow> = sqlx::query_as(
        "SELECT uuid, title, created, updated FROM dev_caloptreyx_aiassistant_conversations
         WHERE user_uuid = $1 AND server_uuid IS NOT DISTINCT FROM $2
         ORDER BY updated DESC LIMIT 100",
    )
    .bind(user)
    .bind(server)
    .fetch_all(db.read())
    .await?;
    Ok(rows.into_iter().map(Conversation::from).collect())
}

pub async fn conversation(
    db: &Database,
    user: Uuid,
    server: Option<Uuid>,
    uuid: Uuid,
) -> Result<Option<Conversation>, sqlx::Error> {
    let row: Option<ConversationRow> = sqlx::query_as(
        "SELECT uuid, title, created, updated FROM dev_caloptreyx_aiassistant_conversations
         WHERE uuid = $1 AND user_uuid = $2 AND server_uuid IS NOT DISTINCT FROM $3",
    )
    .bind(uuid)
    .bind(user)
    .bind(server)
    .fetch_optional(db.read())
    .await?;
    Ok(row.map(Conversation::from))
}

pub async fn create_conversation(
    db: &Database,
    user: Uuid,
    server: Option<Uuid>,
    title: &str,
) -> Result<Conversation, sqlx::Error> {
    let row: ConversationRow = sqlx::query_as(
        "INSERT INTO dev_caloptreyx_aiassistant_conversations (user_uuid, server_uuid, title)
         VALUES ($1, $2, $3)
         RETURNING uuid, title, created, updated",
    )
    .bind(user)
    .bind(server)
    .bind(title)
    .fetch_one(db.write())
    .await?;
    Ok(row.into())
}

/// Marks the conversation as used now; the first message also names it.
pub async fn touch_conversation(
    db: &Database,
    uuid: Uuid,
    title: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_caloptreyx_aiassistant_conversations
         SET updated = now(), title = COALESCE($2, title)
         WHERE uuid = $1",
    )
    .bind(uuid)
    .bind(title)
    .execute(db.write())
    .await?;
    Ok(())
}

pub async fn delete_conversation(db: &Database, uuid: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM dev_caloptreyx_aiassistant_conversations WHERE uuid = $1")
        .bind(uuid)
        .execute(db.write())
        .await?;
    Ok(())
}

pub async fn prune_conversations(db: &Database, days: u32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM dev_caloptreyx_aiassistant_conversations
         WHERE updated < now() - make_interval(days => $1)",
    )
    .bind(days.min(36_500) as i32)
    .execute(db.write())
    .await?;
    Ok(result.rows_affected())
}

#[derive(Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub entry: Entry,
    pub created: DateTime<Utc>,
}

pub async fn messages(db: &Database, conversation: Uuid) -> Result<Vec<Message>, sqlx::Error> {
    let rows: Vec<(i64, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, entry, created FROM dev_caloptreyx_aiassistant_messages
         WHERE conversation_uuid = $1 ORDER BY id",
    )
    .bind(conversation)
    .fetch_all(db.read())
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, entry, created)| match serde_json::from_str(&entry) {
            Ok(entry) => Some(Message { id, entry, created }),
            Err(err) => {
                tracing::warn!("ai assistant: skipping unreadable message {id}: {err}");
                None
            }
        })
        .collect())
}

pub async fn append(
    db: &Database,
    conversation: Uuid,
    entry: Entry,
) -> Result<Message, anyhow::Error> {
    let (id, created): (i64, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO dev_caloptreyx_aiassistant_messages (conversation_uuid, entry)
         VALUES ($1, $2)
         RETURNING id, created",
    )
    .bind(conversation)
    .bind(serde_json::to_string(&entry)?)
    .fetch_one(db.write())
    .await?;
    Ok(Message { id, entry, created })
}

#[derive(Clone, Debug, Serialize)]
pub struct Action {
    pub uuid: Uuid,
    pub seq: i64,
    pub call_id: String,
    pub tool: String,
    /// The tool's title, for the action card.
    pub title: String,
    pub arguments: Value,
    /// `pending`, `running`, `succeeded`, `failed` or `rejected`.
    pub status: String,
    pub result: Option<String>,
    pub created: DateTime<Utc>,
    pub decided: Option<DateTime<Utc>>,
    /// `unavailable`, `available`, `rolling_back`, `rolled_back` or `failed`.
    pub rollback_status: String,
    pub rollback_summary: Option<String>,
    pub rollback_result: Option<String>,
    pub rolled_back: Option<DateTime<Utc>>,
}

type ActionRow = (
    Uuid,
    i64,
    String,
    String,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    String,
    Option<String>,
    Option<String>,
    Option<DateTime<Utc>>,
);

const ACTION_COLUMNS: &str = "uuid, seq, call_id, tool, arguments, status, result, created, decided, rollback_status, rollback_summary, rollback_result, rolled_back";

impl From<ActionRow> for Action {
    fn from(
        (
            uuid,
            seq,
            call_id,
            tool,
            arguments,
            status,
            result,
            created,
            decided,
            rollback_status,
            rollback_summary,
            rollback_result,
            rolled_back,
        ): ActionRow,
    ) -> Self {
        Self {
            uuid,
            seq,
            call_id,
            title: crate::tools::find(&tool)
                .map(|tool| tool.title.to_string())
                .unwrap_or_else(|| tool.clone()),
            tool,
            arguments: serde_json::from_str(&arguments).unwrap_or(Value::String(arguments)),
            status,
            result,
            created,
            decided,
            rollback_status,
            rollback_summary,
            rollback_result,
            rolled_back,
        }
    }
}

pub async fn actions(db: &Database, conversation: Uuid) -> Result<Vec<Action>, sqlx::Error> {
    let rows: Vec<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ACTION_COLUMNS} FROM dev_caloptreyx_aiassistant_actions
         WHERE conversation_uuid = $1 ORDER BY seq ASC"
    )))
    .bind(conversation)
    .fetch_all(db.read())
    .await?;
    Ok(rows.into_iter().map(Action::from).collect())
}

pub async fn action(
    db: &Database,
    conversation: Uuid,
    uuid: Uuid,
) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ACTION_COLUMNS} FROM dev_caloptreyx_aiassistant_actions
         WHERE uuid = $1 AND conversation_uuid = $2"
    )))
    .bind(uuid)
    .bind(conversation)
    .fetch_optional(db.read())
    .await?;
    Ok(row.map(Action::from))
}

pub async fn actions_from_seq_desc(
    db: &Database,
    conversation: Uuid,
    min_seq: i64,
) -> Result<Vec<Action>, sqlx::Error> {
    let rows: Vec<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ACTION_COLUMNS} FROM dev_caloptreyx_aiassistant_actions
         WHERE conversation_uuid = $1 AND seq >= $2
         ORDER BY seq DESC"
    )))
    .bind(conversation)
    .bind(min_seq)
    .fetch_all(db.read())
    .await?;
    Ok(rows.into_iter().map(Action::from).collect())
}

pub async fn create_action(
    db: &Database,
    conversation: Uuid,
    call: &ToolCall,
) -> Result<Action, sqlx::Error> {
    let row: ActionRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO dev_caloptreyx_aiassistant_actions (conversation_uuid, call_id, tool, arguments)
         VALUES ($1, $2, $3, $4)
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(conversation)
    .bind(&call.id)
    .bind(&call.name)
    .bind(call.arguments.to_string())
    .fetch_one(db.write())
    .await?;
    Ok(row.into())
}

/// Moves a pending action to `status` (`running` or `rejected`). Returns `None` when it is not
/// pending (any more), so two clicks can never run it twice.
pub async fn decide_action(
    db: &Database,
    conversation: Uuid,
    uuid: Uuid,
    status: &str,
) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET status = $3, decided = now()
         WHERE uuid = $1 AND conversation_uuid = $2 AND status = 'pending'
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(uuid)
    .bind(conversation)
    .bind(status)
    .fetch_optional(db.write())
    .await?;
    Ok(row.map(Action::from))
}

pub async fn finish_action(
    db: &Database,
    uuid: Uuid,
    status: &str,
    result: &str,
) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE dev_caloptreyx_aiassistant_actions SET status = $2, result = $3
         WHERE uuid = $1
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(uuid)
    .bind(status)
    .bind(result)
    .fetch_optional(db.write())
    .await?;
    Ok(row.map(Action::from))
}

/// Atomically finishes a confirmed action and, when it succeeded with a captured snapshot, stores
/// that snapshot and marks rollback as available.
pub async fn finish_action_with_rollback(
    db: &Database,
    uuid: Uuid,
    status: &str,
    result: &str,
    rollback_status: &str,
    rollback_summary: Option<&str>,
    snapshot: Option<(&str, &str)>,
) -> Result<Option<Action>, sqlx::Error> {
    let mut tx = db.write().begin().await?;
    if status == "succeeded"
        && let Some((kind, payload)) = snapshot
    {
        sqlx::query(
            "INSERT INTO dev_caloptreyx_aiassistant_snapshots (action_uuid, kind, payload)
             VALUES ($1, $2, $3)
             ON CONFLICT (action_uuid) DO UPDATE SET
                 kind = EXCLUDED.kind,
                 payload = EXCLUDED.payload",
        )
        .bind(uuid)
        .bind(kind)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    }
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET status = $2, result = $3, rollback_status = $4, rollback_summary = $5
         WHERE uuid = $1
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(uuid)
    .bind(status)
    .bind(result)
    .bind(rollback_status)
    .bind(rollback_summary)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(Action::from))
}

pub async fn load_snapshot(
    db: &Database,
    action_uuid: Uuid,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT kind, payload FROM dev_caloptreyx_aiassistant_snapshots
         WHERE action_uuid = $1",
    )
    .bind(action_uuid)
    .fetch_optional(db.read())
    .await
}

/// Atomically transitions a reversible action from `available` or `failed` to `rolling_back`.
pub async fn begin_rollback(
    db: &Database,
    conversation: Uuid,
    uuid: Uuid,
) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET rollback_status = 'rolling_back', rolled_back = now()
         WHERE uuid = $1 AND conversation_uuid = $2
             AND status = 'succeeded'
             AND rollback_status IN ('available', 'failed')
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(uuid)
    .bind(conversation)
    .fetch_optional(db.write())
    .await?;
    Ok(row.map(Action::from))
}

/// Reverts `rolling_back` back to `available` when a non-forced conflict stops the rollback.
pub async fn revert_rollback(db: &Database, uuid: Uuid) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET rollback_status = 'available', rolled_back = NULL
         WHERE uuid = $1
         RETURNING {ACTION_COLUMNS}"
    )))
    .bind(uuid)
    .fetch_optional(db.write())
    .await?;
    Ok(row.map(Action::from))
}

/// Records the outcome of a rollback attempt (`rolled_back` or `failed`).
pub async fn finish_rollback(
    db: &Database,
    uuid: Uuid,
    rollback_status: &str,
    rollback_result: &str,
) -> Result<Option<Action>, sqlx::Error> {
    let row: Option<ActionRow> = if rollback_status == "rolled_back" {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE dev_caloptreyx_aiassistant_actions
             SET rollback_status = $2, rollback_result = $3, rolled_back = now()
             WHERE uuid = $1
             RETURNING {ACTION_COLUMNS}"
        )))
        .bind(uuid)
        .bind(rollback_status)
        .bind(rollback_result)
        .fetch_optional(db.write())
        .await?
    } else {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE dev_caloptreyx_aiassistant_actions
             SET rollback_status = $2, rollback_result = $3, rolled_back = NULL
             WHERE uuid = $1
             RETURNING {ACTION_COLUMNS}"
        )))
        .bind(uuid)
        .bind(rollback_status)
        .bind(rollback_result)
        .fetch_optional(db.write())
        .await?
    };
    Ok(row.map(Action::from))
}

/// Actions still running a long time after they were confirmed were cut off by a restart.
pub async fn fail_interrupted_actions(db: &Database, conversation: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET status = 'failed', result = 'Interrupted: the panel restarted while this action ran.'
         WHERE conversation_uuid = $1 AND status = 'running'
             AND decided < now() - interval '15 minutes'",
    )
    .bind(conversation)
    .execute(db.write())
    .await?;
    Ok(())
}

/// Rollbacks still running a long time after they started were cut off by a restart.
pub async fn fail_interrupted_rollbacks(
    db: &Database,
    conversation: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_caloptreyx_aiassistant_actions
         SET rollback_status = 'failed',
             rollback_result = 'Interrupted: the panel restarted while this rollback ran.',
             rolled_back = NULL
         WHERE conversation_uuid = $1 AND rollback_status = 'rolling_back'
             AND COALESCE(rolled_back, decided, created) < now() - interval '15 minutes'",
    )
    .bind(conversation)
    .execute(db.write())
    .await?;
    Ok(())
}

pub async fn record_usage(
    db: &Database,
    user: Uuid,
    source: KeySource,
    requests: i32,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dev_caloptreyx_aiassistant_usage
             (user_uuid, day, source, requests, input_tokens, output_tokens)
         VALUES ($1, CURRENT_DATE, $2, $3, $4, $5)
         ON CONFLICT (user_uuid, day, source) DO UPDATE SET
             requests = dev_caloptreyx_aiassistant_usage.requests + EXCLUDED.requests,
             input_tokens = dev_caloptreyx_aiassistant_usage.input_tokens + EXCLUDED.input_tokens,
             output_tokens = dev_caloptreyx_aiassistant_usage.output_tokens + EXCLUDED.output_tokens",
    )
    .bind(user)
    .bind(source.as_str())
    .bind(requests)
    .bind(input_tokens.min(i64::MAX as u64) as i64)
    .bind(output_tokens.min(i64::MAX as u64) as i64)
    .execute(db.write())
    .await?;
    Ok(())
}

/// Tokens (input plus output) the user used today, per key that paid.
pub async fn tokens_today(db: &Database, user: Uuid, source: KeySource) -> Result<u64, sqlx::Error> {
    let tokens: Option<i64> = sqlx::query_scalar(
        "SELECT input_tokens + output_tokens FROM dev_caloptreyx_aiassistant_usage
         WHERE user_uuid = $1 AND day = CURRENT_DATE AND source = $2",
    )
    .bind(user)
    .bind(source.as_str())
    .fetch_optional(db.read())
    .await?;
    Ok(tokens.unwrap_or(0).max(0) as u64)
}

pub async fn rename_conversation(db: &Database, uuid: Uuid, title: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_caloptreyx_aiassistant_conversations SET title = $2 WHERE uuid = $1",
    )
    .bind(uuid)
    .bind(title)
    .execute(db.write())
    .await?;
    Ok(())
}

/// A user's stored provider key; the key itself stays encrypted until it is used.
pub struct StoredUserKey {
    pub preset: String,
    pub base_url: Option<String>,
    pub model: String,
    pub api_key_encrypted: String,
    pub updated: DateTime<Utc>,
}

pub async fn user_key(db: &Database, user: Uuid) -> Result<Option<StoredUserKey>, sqlx::Error> {
    let row: Option<(String, Option<String>, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT preset, base_url, model, api_key_encrypted, updated
         FROM dev_caloptreyx_aiassistant_user_keys WHERE user_uuid = $1",
    )
    .bind(user)
    .fetch_optional(db.read())
    .await?;
    Ok(row.map(
        |(preset, base_url, model, api_key_encrypted, updated)| StoredUserKey {
            preset,
            base_url,
            model,
            api_key_encrypted,
            updated,
        },
    ))
}

/// Stores a user's key. `api_key_encrypted` is `None` to keep the stored key.
pub async fn save_user_key(
    db: &Database,
    user: Uuid,
    preset: &str,
    base_url: Option<&str>,
    model: &str,
    api_key_encrypted: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dev_caloptreyx_aiassistant_user_keys
             (user_uuid, preset, base_url, model, api_key_encrypted)
         VALUES ($1, $2, $3, $4, COALESCE($5, ''))
         ON CONFLICT (user_uuid) DO UPDATE SET
             preset = EXCLUDED.preset,
             base_url = EXCLUDED.base_url,
             model = EXCLUDED.model,
             api_key_encrypted = COALESCE($5, dev_caloptreyx_aiassistant_user_keys.api_key_encrypted),
             updated = now()",
    )
    .bind(user)
    .bind(preset)
    .bind(base_url)
    .bind(model)
    .bind(api_key_encrypted)
    .execute(db.write())
    .await?;
    Ok(())
}

pub async fn delete_user_key(db: &Database, user: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM dev_caloptreyx_aiassistant_user_keys WHERE user_uuid = $1")
        .bind(user)
        .execute(db.write())
        .await?;
    Ok(())
}
