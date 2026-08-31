use crate::{
    api::node_enroll,
    cmdb::{
        assets::MutationContext,
        contracts::InventorySnapshotV1,
        observations::{self, IngestSnapshotInput, ObservationError},
    },
    error::{AppError, Result},
    operations::contracts::{ActorRef, ActorType},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::Value;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
fn map_error(error: ObservationError) -> AppError {
    match error {
        ObservationError::Invalid(message) => AppError::BadRequest(message),
        ObservationError::SourceNotFound
        | ObservationError::AssetNotFound
        | ObservationError::ObservationNotFound => AppError::NotFound,
        ObservationError::NodeNotTrusted => AppError::Unauthorized,
        ObservationError::Conflict(_) => {
            AppError::Conflict("inventory snapshot conflicts with an existing snapshot".into())
        }
        ObservationError::Processing => {
            AppError::Conflict("inventory snapshot is still processing".into())
        }
        ObservationError::Internal(_) => {
            AppError::Internal(anyhow::anyhow!("inventory operation failed"))
        }
    }
}
pub async fn upload(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>> {
    if body.len() > MAX_BODY_BYTES {
        return Err(AppError::BadRequest(
            "inventory snapshot exceeds 4 MiB".into(),
        ));
    }
    node_enroll::verify_node_token(state.clone(), node_id.clone(), headers.clone()).await?;
    let snapshot: InventorySnapshotV1 = serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("invalid inventory snapshot".into()))?;
    let source_resource_id: Option<String> = sqlx::query_scalar("SELECT r.id FROM resources r JOIN cmdb_assets a ON a.resource_id = r.id WHERE r.node_id = ? AND r.kind = ? ORDER BY r.id LIMIT 1").bind(&node_id).bind("cmdb_asset").fetch_optional(&state.db).await.map_err(AppError::Database)?;
    let source_resource_id = source_resource_id.ok_or(AppError::NotFound)?;
    let actor = ActorRef {
        actor_type: ActorType::Node,
        id: Some(node_id.clone()),
        source: Some("inventory_agent".into()),
    };
    let input = IngestSnapshotInput {
        source_resource_id,
        node_id: Some(node_id.clone()),
        provider: "agent".into(),
        snapshot,
    };
    let result = observations::ingest(
        &state.db,
        input,
        MutationContext {
            actor,
            correlation_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
    .map_err(map_error)?;
    Ok(Json(serde_json::to_value(result).map_err(|_| {
        AppError::Internal(anyhow::anyhow!("response encoding failed"))
    })?))
}
