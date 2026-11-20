use indexmap::IndexMap;
use shared::{
    State,
    extensions::{
        Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder, ExtensionUpdateInfo,
        background_tasks::BackgroundTaskBuilder, settings::ExtensionSettingsDeserializer,
    },
    permissions::PermissionGroup,
};
use std::{sync::Arc, time::Duration};

mod access;
mod chat;
mod db;
mod openapi;
mod provider;
mod proxy;
mod routes;
mod settings;
mod tools;
mod upstream;

pub const PACKAGE: &str = "dev.caloptreyx.aiassistant";
/// Where the server chat is mounted below `/api/client/servers/{server}`.
pub const SERVER_ROUTE: &str = "ai-assistant";

const LATEST_RELEASE: &str = "https://api.github.com/repos/Caloptreyx/AI-Assistant/releases/latest";

/// The subset of GitHub's release payload the updates page needs.
#[derive(serde::Serialize, serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
}

fn version_from_tag(tag: &str) -> Result<semver::Version, semver::Error> {
    semver::Version::parse(tag.trim_start_matches('v'))
}

/// Deletes conversations nobody touched within the retention period, once an hour.
async fn prune(state: &State) -> Result<(), anyhow::Error> {
    let days = settings::load(state).await?.retention_days;
    if days > 0 {
        let pruned = db::prune_conversations(&state.database, days).await?;
        if pruned > 0 {
            tracing::info!("ai assistant: pruned {pruned} old conversations");
        }
    }
    tokio::time::sleep(Duration::from_secs(3600)).await;
    Ok(())
}

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder
            .add_client_server_api_router(|router| {
                router.nest(
                    &format!("/{SERVER_ROUTE}"),
                    routes::server::router(&state),
                )
            })
            .add_admin_api_router(|router| {
                router.nest(
                    &format!("/extensions/{PACKAGE}"),
                    routes::admin::router(&state),
                )
            })
    }

    async fn initialize_permissions(
        &mut self,
        _state: State,
        builder: ExtensionPermissionsBuilder,
    ) -> ExtensionPermissionsBuilder {
        builder
            .add_server_permission_group(
                "ai-assistant",
                PermissionGroup {
                    description: "Permissions for the AI assistant on this server.",
                    permissions: IndexMap::from([(
                        "chat",
                        "Allows chatting with the AI assistant about this server. It can only read and change what the other permissions allow, and every change needs a confirmation.",
                    )]),
                },
            )
            .add_admin_permission_group(
                "ai-assistant",
                PermissionGroup {
                    description: "Permissions for the AI assistant in the admin area.",
                    permissions: IndexMap::from([(
                        "chat",
                        "Allows using the AI assistant in the admin area. It can only read and change what the other admin permissions allow, and every change needs a confirmation.",
                    )]),
                },
            )
    }

    async fn initialize_background_tasks(
        &mut self,
        _state: State,
        builder: BackgroundTaskBuilder,
    ) -> BackgroundTaskBuilder {
        builder
            .add_task("ai-assistant-prune", |state| async move { prune(&state).await })
            .await;
        builder
    }

    async fn settings_deserializer(&self, _state: State) -> ExtensionSettingsDeserializer {
        Arc::new(settings::SettingsDataDeserializer)
    }

    async fn check_for_updates(
        &self,
        state: State,
        current_version: &semver::Version,
    ) -> Result<Option<ExtensionUpdateInfo>, anyhow::Error> {
        // GitHub allows 60 unauthenticated requests per hour, and the Panel
        // rechecks on every start and every 12 hours.
        let release: Release = state
            .cache
            .cached("dev.caloptreyx.aiassistant::latest-release", 60 * 60, || async {
                let response = state.client.get(LATEST_RELEASE).send().await?;
                Ok::<Release, anyhow::Error>(response.error_for_status()?.json::<Release>().await?)
            })
            .await?;

        let latest = version_from_tag(&release.tag_name)?;

        Ok((latest > *current_version).then(|| ExtensionUpdateInfo {
            version: latest,
            changes: release
                .body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(compact_str::CompactString::from)
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_tag_accepts_prefixed_and_bare_tags() {
        assert_eq!(version_from_tag("v0.1.0").unwrap().to_string(), "0.1.0");
        assert_eq!(version_from_tag("1.2.3").unwrap().to_string(), "1.2.3");
        assert!(version_from_tag("release-1").is_err());
    }
}
