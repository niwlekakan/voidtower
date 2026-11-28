use crate::{
    api::mcp::test_support,
    cmdb::{
        assets::{self, CreateAssetInput, MutationContext},
        observations::ObservationRecord,
    },
    operations::contracts::{ActorRef, ActorType},
};
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    response::Response,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tower::ServiceExt;

async fn setup() -> (SqlitePool, axum::Router) {
    let db = test_support::setup_db().await;
    let mut transaction = db.begin().await.unwrap();
    crate::cmdb::catalog::seed(&mut transaction, 1)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let app = crate::api::router(test_support::build(db.clone()));
    (db, app)
}

async fn session(db: &SqlitePool, role: &str) -> String {
    test_support::user_with_role_session(db, role).await
}

async fn bearer_token(db: &SqlitePool) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
         VALUES (?, ?, 'x', 'owner', 0, 0)",
    )
    .bind(&user_id)
    .bind(format!("cmdb-token-{user_id}"))
    .execute(db)
    .await
    .unwrap();
    let raw = format!("vt_cmdb_{}", uuid::Uuid::new_v4().simple());
    let mut hash = Sha256::new();
    hash.update(raw.as_bytes());
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at) \
         VALUES (?, ?, 'cmdb-test', ?, '[\"admin:write\"]', 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(hex::encode(hash.finalize()))
    .execute(db)
    .await
    .unwrap();
    raw
}

fn request(method: Method, uri: &str, session: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("vt_session={session}"));
    }
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap()
}

async fn send(
    app: &axum::Router,
    method: Method,
    uri: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> Response {
    app.clone()
        .oneshot(request(method, uri, session, body))
        .await
        .unwrap()
}

async fn send_raw(
    app: &axum::Router,
    method: Method,
    uri: &str,
    session: Option<&str>,
    body: impl Into<Body>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("vt_session={session}"));
    }
    app.clone()
        .oneshot(builder.body(body.into()).unwrap())
        .await
        .unwrap()
}

async fn send_node_raw(
    app: &axum::Router,
    uri: &str,
    token: &str,
    body: impl Into<Body>,
) -> Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn json_body(response: Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn assert_error(response: Response, status: StatusCode, code: &str) -> Value {
    assert_eq!(response.status(), status);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], code);
    assert!(body["error"]["message"].as_str().is_some());
    body
}

fn context() -> MutationContext {
    MutationContext {
        actor: ActorRef {
            actor_type: ActorType::Human,
            id: Some("test-owner".into()),
            source: Some("cmdb_api_test".into()),
        },
        correlation_id: uuid::Uuid::new_v4().to_string(),
    }
}

async fn asset(
    db: &SqlitePool,
    class_key: &str,
    type_key: &str,
    name: &str,
) -> crate::cmdb::contracts::AssetRecord {
    assets::create_manual(
        db,
        CreateAssetInput {
            class_key: class_key.into(),
            type_key: type_key.into(),
            name: name.into(),
            friendly_name: None,
            description: None,
            manufacturer: None,
            model: None,
            serial_number: None,
            part_number: None,
            location_id: None,
            metadata: json!({}),
            notes: String::new(),
        },
        context(),
    )
    .await
    .unwrap()
}

async fn enrolled_agent_host(db: &SqlitePool) -> (String, String, String) {
    let node_id = format!("inventory-node-{}", uuid::Uuid::new_v4());
    let raw_token = format!("vt_node_{}", uuid::Uuid::new_v4().simple());
    sqlx::query("INSERT OR IGNORE INTO users (id, username, password_hash, role, created_at, updated_at) VALUES ('inventory-owner', 'inventory-owner', 'x', 'owner', 0, 0)")
        .execute(db).await.unwrap();
    sqlx::query("INSERT INTO nodes (id, display_name, device_type, owner_user_id, wg_peer_id, wg_public_key, token_hash, agent_capable, approved, created_at) VALUES (?, 'fixture agent', 'other', 'inventory-owner', NULL, '', ?, 1, 1, 0)")
        .bind(&node_id)
        .bind(crate::api::integrations::sha256_hex(&raw_token))
        .execute(db).await.unwrap();
    let host = asset(db, "sys", "host", "Fixture host").await;
    sqlx::query("UPDATE resources SET node_id = ? WHERE id = ?")
        .bind(&node_id)
        .bind(&host.resource_id)
        .execute(db)
        .await
        .unwrap();
    (node_id, raw_token, host.resource_id)
}

fn inventory_snapshot(snapshot_id: &str, host_name: &str) -> Value {
    json!({"schema_version":1,"snapshot_id":snapshot_id,"collector_version":"fixture-0.1","platform":"linux","collected_at":1700000000,"host":{"entity_key":"host","identities":[{"kind":"hardware_uuid","value":host_name}],"attributes":{"hostname":host_name},"runtime":{"kernel":"fixture"}},"entities":[]})
}

fn linux_collector_snapshot() -> Value {
    let raw = r#"{"blockdevices":[{"name":"sda","type":"disk","size":100,"model":"Fixture Disk","serial":"SERIAL-001","wwn":"0011223344556677","rota":true,"tran":"sata","rm":false,"ro":false,"path":"/dev/sda","mountpoints":[null]},{"name":"sda1","type":"part"},{"name":"loop0","type":"loop"},{"name":"zram0","type":"ram"}]}"#;
    serde_json::to_value(crate::collector::collect_linux_fixture(raw).unwrap()).unwrap()
}

async fn insert_discovery(
    db: &SqlitePool,
    source_resource_id: &str,
    fingerprint: &str,
    entity_key: &str,
) -> ObservationRecord {
    let snapshot_row_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO cmdb_inventory_snapshots \
         (id, source_resource_id, node_id, snapshot_id, schema_version, collector_version, platform, \
          collected_at, received_at, state, fingerprint, result_json) \
         VALUES (?, ?, NULL, ?, 1, 'test', 'linux', 10, 10, 'completed', ?, '{}')",
    )
    .bind(&snapshot_row_id)
    .bind(source_resource_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(format!("snapshot-{fingerprint}"))
    .execute(db)
    .await
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO cmdb_observations \
         (id, resource_id, source_resource_id, snapshot_row_id, provider, scope_key, entity_key, \
          entity_type, schema_version, identity_json, attributes_json, runtime_json, health_json, \
          provider_observed_at, received_at, first_seen_at, last_seen_at, state, fingerprint) \
         VALUES (?, NULL, ?, ?, 'manual_test', 'test', ?, 'physical_disk', 1, \
                 ?, \
                 '{\"model\":\"Review Disk\",\"rotation\":true}', '{}', '{}', 10, 10, 10, 10, \
                 'review', ?)",
    )
    .bind(&id)
    .bind(source_resource_id)
    .bind(&snapshot_row_id)
    .bind(entity_key)
    .bind(
        serde_json::to_string(&json!([{
            "kind": "serial_model",
            "value": format!("test-identity-{entity_key}"),
            "confidence": "strong"
        }]))
        .unwrap(),
    )
    .bind(fingerprint)
    .execute(db)
    .await
    .unwrap();
    crate::cmdb::observations::get_observation(db, &id)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn inventory_upload_authenticates_before_rejecting_oversized_body() {
    let (db, app) = setup().await;
    let oversized = "x".repeat(4 * 1024 * 1024 + 2);
    assert_error(
        send_raw(
            &app,
            Method::POST,
            "/api/nodes/missing/inventory",
            None,
            oversized,
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;

    let (node_id, token, _) = enrolled_agent_host(&db).await;
    assert_error(
        send_node_raw(
            &app,
            &format!("/api/nodes/{node_id}/inventory"),
            &token,
            "x".repeat(4 * 1024 * 1024 + 2),
        )
        .await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    )
    .await;
}

#[tokio::test]
async fn inventory_upload_rejects_semantically_invalid_snapshots() {
    let (db, app) = setup().await;
    let (node_id, token, host_id) = enrolled_agent_host(&db).await;
    let mut invalid = inventory_snapshot(&uuid::Uuid::new_v4().to_string(), "fixture-host");
    invalid["schema_version"] = json!(2);

    assert_error(
        send_node_raw(
            &app,
            &format!("/api/nodes/{node_id}/inventory"),
            &token,
            invalid.to_string(),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    )
    .await;

    let mut colliding = inventory_snapshot(&uuid::Uuid::new_v4().to_string(), "fixture-host");
    colliding["entities"] = json!([{ "entity_key": "host", "entity_type": "physical_disk" }]);
    assert_error(
        send_node_raw(
            &app,
            &format!("/api/nodes/{node_id}/inventory"),
            &token,
            colliding.to_string(),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    )
    .await;

    let mut invalid_capacity =
        inventory_snapshot(&uuid::Uuid::new_v4().to_string(), "fixture-host");
    invalid_capacity["entities"] = json!([{
        "entity_key": "disk-1",
        "entity_type": "physical_disk",
        "identities": [{"kind": "serial", "value": "SERIAL-INVALID-CAPACITY"}],
        "attributes": {"capacity_bytes": 0, "rotation": false, "protocol": "sata"}
    }]);
    assert_error(
        send_node_raw(
            &app,
            &format!("/api/nodes/{node_id}/inventory"),
            &token,
            invalid_capacity.to_string(),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cmdb_inventory_snapshots WHERE source_resource_id = ?"
        )
        .bind(host_id)
        .fetch_one(&db)
        .await
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn inventory_upload_is_authenticated_idempotent_and_binds_the_node_host() {
    let (db, app) = setup().await;
    let (node_id, token, host_id) = enrolled_agent_host(&db).await;
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let body = inventory_snapshot(&format!(" {snapshot_id} "), "fixture-host");

    let mut first = request(
        Method::POST,
        &format!("/api/nodes/{node_id}/inventory"),
        None,
        Some(body.clone()),
    );
    first.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    let response = app.clone().oneshot(first).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = json_body(response).await;
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["result"]["snapshot_id"], snapshot_id);
    assert_eq!(result["result"]["replayed"], false);

    let mut replay = request(
        Method::POST,
        &format!("/api/nodes/{node_id}/inventory"),
        None,
        Some(inventory_snapshot(&snapshot_id, "fixture-host")),
    );
    replay.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    let replayed = json_body(app.clone().oneshot(replay).await.unwrap()).await;
    assert_eq!(replayed["result"]["replayed"], true);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cmdb_inventory_snapshots WHERE source_resource_id = ?"
        )
        .bind(&host_id)
        .fetch_one(&db)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn linux_collector_snapshot_reaches_reconciliation_classification() {
    let (db, app) = setup().await;
    let (node_id, token, host_id) = enrolled_agent_host(&db).await;
    let response = send_node_raw(
        &app,
        &format!("/api/nodes/{node_id}/inventory"),
        &token,
        linux_collector_snapshot().to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = json_body(response).await;
    assert_eq!(result["result"]["registered"], 1);
    assert_eq!(result["result"]["review_required"], 0);

    let (attributes, identities): (String, String) = sqlx::query_as(
        "SELECT attributes_json, identity_json FROM cmdb_observations WHERE source_resource_id = ? AND entity_type = 'physical_disk'",
    )
    .bind(&host_id)
    .fetch_one(&db)
    .await
    .unwrap();
    let attributes: Value = serde_json::from_str(&attributes).unwrap();
    assert_eq!(attributes["protocol"], "sata");
    assert_eq!(attributes["rotation"], true);
    assert_eq!(attributes["serial"], "SERIAL-001");
    assert_eq!(attributes["wwn"], "0011223344556677");
    assert_eq!(attributes["capacity_bytes"], 100);
    assert!(attributes.get("size_bytes").is_none());
    let identities: Value = serde_json::from_str(&identities).unwrap();
    assert!(identities
        .as_array()
        .unwrap()
        .iter()
        .any(|identity| { identity["kind"] == "wwn" && identity["value"] == "0011223344556677" }));

    let (request_id, audit_actor_type, audit_actor_id, audit_source): (
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT request_id, actor_type, user_id, source FROM audit_log \
         WHERE action = 'cmdb.inventory.ingest' \
         ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(audit_actor_type, "node");
    assert_eq!(audit_actor_id.as_deref(), Some(node_id.as_str()));
    assert_eq!(audit_source.as_deref(), Some("inventory_agent"));
    let (correlation_id, actor_type, actor_id, actor_source, payload): (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ) = sqlx::query_as(
        "SELECT correlation_id, actor_type, actor_id, actor_source, payload_json \
         FROM events WHERE event_type = 'cmdb.asset.created.v1' AND resource_id IN \
           (SELECT resource_id FROM cmdb_observations WHERE source_resource_id = ? AND entity_type = 'physical_disk') \
         ORDER BY sequence DESC LIMIT 1",
    )
    .bind(&host_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(request_id.as_deref(), Some(correlation_id.as_str()));
    assert_eq!(actor_type, "node");
    assert_eq!(actor_id.as_deref(), Some(node_id.as_str()));
    assert_eq!(actor_source.as_deref(), Some("inventory_agent"));
    assert!(!payload.contains(&token));
}

#[tokio::test]
async fn empty_inventory_snapshot_does_not_mark_existing_inventory_missing() {
    let (db, app) = setup().await;
    let (node_id, token, host_id) = enrolled_agent_host(&db).await;
    let first = send_node_raw(
        &app,
        &format!("/api/nodes/{node_id}/inventory"),
        &token,
        linux_collector_snapshot().to_string(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(json_body(first).await["result"]["registered"], 1);

    let snapshot = inventory_snapshot(&uuid::Uuid::new_v4().to_string(), "fixture-host");
    let empty = send_node_raw(
        &app,
        &format!("/api/nodes/{node_id}/inventory"),
        &token,
        snapshot.to_string(),
    )
    .await;
    assert_eq!(empty.status(), StatusCode::OK);
    let result = json_body(empty).await;
    assert_eq!(result["result"]["missing"], 0);
    assert_eq!(result["result"]["linked"], 0);
    assert_eq!(result["result"]["registered"], 0);

    let (observation_state, discovery_status): (String, String) = sqlx::query_as(
        "SELECT o.state, a.discovery_status FROM cmdb_observations o \
         JOIN cmdb_assets a ON a.resource_id = o.resource_id \
         WHERE o.source_resource_id = ? AND o.entity_type = 'physical_disk'",
    )
    .bind(&host_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(observation_state, "online");
    assert_eq!(discovery_status, "online");
}

#[tokio::test]
async fn inventory_reconciliation_preserves_administrator_owned_asset_fields() {
    let (db, app) = setup().await;
    let (node_id, token, host_id) = enrolled_agent_host(&db).await;
    let first = send_node_raw(
        &app,
        &format!("/api/nodes/{node_id}/inventory"),
        &token,
        linux_collector_snapshot().to_string(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let disk_id: String = sqlx::query_scalar(
        "SELECT resource_id FROM cmdb_observations WHERE source_resource_id = ? AND entity_type = 'physical_disk'",
    )
    .bind(&host_id)
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE resources SET display_name = ? WHERE id = ?")
        .bind("Admin disk title")
        .bind(&disk_id)
        .execute(&db)
        .await
        .unwrap();
    let location_id = "admin-location";
    sqlx::query(
        "INSERT INTO cmdb_locations (id, parent_id, name, description, created_at, updated_at) \
         VALUES (?, NULL, ?, NULL, 0, 0)",
    )
    .bind(location_id)
    .bind("Admin location")
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE cmdb_assets SET friendly_name = ?, description = ?, manufacturer = ?, model = ?, \
         serial_number = ?, part_number = ?, lifecycle_status = 'deployed', condition_status = 'good', \
         location_id = ?, metadata_json = ?, notes = ? WHERE resource_id = ?",
    )
    .bind("Admin friendly")
    .bind("Admin description")
    .bind("Admin maker")
    .bind("Admin model")
    .bind("ADMIN-SERIAL")
    .bind("ADMIN-PART")
    .bind(location_id)
    .bind(r#"{"owner":"admin"}"#)
    .bind("Admin notes")
    .bind(&disk_id)
    .execute(&db)
    .await
    .unwrap();

    let mut followup = linux_collector_snapshot();
    followup["snapshot_id"] = json!(uuid::Uuid::new_v4().to_string());
    let response = send_node_raw(
        &app,
        &format!("/api/nodes/{node_id}/inventory"),
        &token,
        followup.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    type ProtectedFields = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
        String,
    );
    let fields: ProtectedFields = sqlx::query_as(
        "SELECT r.display_name, a.friendly_name, a.description, a.manufacturer, a.model, \
         a.serial_number, a.part_number, a.lifecycle_status, a.condition_status, a.location_id, \
         a.metadata_json, a.notes \
         FROM resources r JOIN cmdb_assets a ON a.resource_id = r.id WHERE r.id = ?",
    )
    .bind(&disk_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        fields,
        (
            "Admin disk title".into(),
            Some("Admin friendly".into()),
            Some("Admin description".into()),
            Some("Admin maker".into()),
            Some("Admin model".into()),
            Some("ADMIN-SERIAL".into()),
            Some("ADMIN-PART".into()),
            "deployed".into(),
            "good".into(),
            Some(location_id.into()),
            r#"{"owner":"admin"}"#.into(),
            "Admin notes".into(),
        )
    );
}

#[tokio::test]
async fn inventory_upload_rejects_wrong_path_conflict_and_revoked_node() {
    let (db, app) = setup().await;
    let (node_id, token, _) = enrolled_agent_host(&db).await;
    let other_node = format!("other-inventory-node-{}", uuid::Uuid::new_v4());
    let other_token = format!("vt_node_{}", uuid::Uuid::new_v4().simple());
    sqlx::query("INSERT INTO nodes (id, display_name, device_type, owner_user_id, wg_peer_id, wg_public_key, token_hash, agent_capable, approved, created_at) VALUES (?, 'other fixture agent', 'other', 'inventory-owner', NULL, '', ?, 1, 1, 0)")
        .bind(&other_node)
        .bind(crate::api::integrations::sha256_hex(&other_token))
        .execute(&db).await.unwrap();
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let body = inventory_snapshot(&snapshot_id, "fixture-host");

    let mut wrong = request(
        Method::POST,
        &format!("/api/nodes/{other_node}/inventory"),
        None,
        Some(body.clone()),
    );
    wrong.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert_error(
        app.clone().oneshot(wrong).await.unwrap(),
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;

    let mut first = request(
        Method::POST,
        &format!("/api/nodes/{node_id}/inventory"),
        None,
        Some(body),
    );
    first.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert_eq!(
        app.clone().oneshot(first).await.unwrap().status(),
        StatusCode::OK
    );

    let mut conflict = request(
        Method::POST,
        &format!("/api/nodes/{node_id}/inventory"),
        None,
        Some(inventory_snapshot(&snapshot_id, "different-host")),
    );
    conflict.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert_error(
        app.clone().oneshot(conflict).await.unwrap(),
        StatusCode::CONFLICT,
        "conflict",
    )
    .await;

    sqlx::query("UPDATE nodes SET approved = 0 WHERE id = ?")
        .bind(&node_id)
        .execute(&db)
        .await
        .unwrap();
    let mut revoked = request(
        Method::POST,
        &format!("/api/nodes/{node_id}/inventory"),
        None,
        Some(inventory_snapshot(
            &uuid::Uuid::new_v4().to_string(),
            "fixture-host",
        )),
    );
    revoked.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert_error(
        app.oneshot(revoked).await.unwrap(),
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}

#[tokio::test]
async fn cmdb_routes_enforce_operator_reads_admin_writes_and_fail_closed() {
    let (db, app) = setup().await;
    let operator = session(&db, "operator").await;
    let admin = session(&db, "admin").await;
    let member = session(&db, "member").await;
    let unknown = session(&db, "future_role").await;

    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/cmdb/classes",
            Some(&operator),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    for denied in [Some(member.as_str()), Some(unknown.as_str()), None] {
        let expected = if denied.is_some() {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::UNAUTHORIZED
        };
        assert_eq!(
            send(&app, Method::GET, "/api/cmdb/classes", denied, None)
                .await
                .status(),
            expected
        );
    }
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/cmdb/classes",
            Some(&operator),
            Some(json!({"key":"lab","label":"Lab"})),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/cmdb/classes",
            Some(&admin),
            Some(json!({"key":"lab","label":"Lab"})),
        )
        .await
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn cmdb_routes_deny_bearer_credentials_and_bound_mutation_bodies() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;
    let token = bearer_token(&db).await;

    let bearer_only = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/cmdb/classes")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_error(bearer_only, StatusCode::FORBIDDEN, "insufficient_scope").await;

    let malformed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/cmdb/classes")
                .header(header::COOKIE, format!("vt_session={admin}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_error(malformed, StatusCode::BAD_REQUEST, "bad_request").await;

    let oversized = json!({"key":"large","label":"x".repeat(65_537)}).to_string();
    let too_large = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/cmdb/classes")
                .header(header::COOKIE, format!("vt_session={admin}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_error(
        too_large,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    )
    .await;
}

#[tokio::test]
async fn every_cmdb_list_rejects_out_of_range_pagination() {
    let (db, app) = setup().await;
    let operator = session(&db, "operator").await;
    for path in [
        "/api/cmdb/classes?limit=0",
        "/api/cmdb/types?limit=201",
        "/api/cmdb/locations?offset=-1",
        "/api/cmdb/discoveries?limit=0",
        "/api/cmdb/assets/missing/history?limit=201",
        "/api/cmdb/assets/missing/observations?offset=-1",
        "/api/cmdb/assets/missing/relationships?limit=0",
    ] {
        assert_error(
            send(&app, Method::GET, path, Some(&operator), None).await,
            StatusCode::BAD_REQUEST,
            "bad_request",
        )
        .await;
    }
}

#[tokio::test]
async fn every_cmdb_list_maps_malformed_pagination_after_authentication() {
    let (db, app) = setup().await;
    let operator = session(&db, "operator").await;
    for path in [
        "/api/cmdb/assets?limit=invalid",
        "/api/cmdb/classes?limit=invalid",
        "/api/cmdb/types?offset=invalid",
        "/api/cmdb/locations?limit=invalid",
        "/api/cmdb/discoveries?offset=invalid",
        "/api/cmdb/assets/missing/history?limit=invalid",
        "/api/cmdb/assets/missing/observations?offset=invalid",
        "/api/cmdb/assets/missing/relationships?limit=invalid",
    ] {
        assert_error(
            send(&app, Method::GET, path, Some(&operator), None).await,
            StatusCode::BAD_REQUEST,
            "bad_request",
        )
        .await;
        assert_error(
            send(&app, Method::GET, path, None, None).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
}

#[tokio::test]
async fn asset_mutations_map_malformed_and_oversized_bodies_after_authentication() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;
    let routes = [
        "/api/cmdb/assets",
        "/api/cmdb/assets/missing/rename",
        "/api/cmdb/assets/missing/retirement",
    ];

    for route in routes {
        assert_error(
            send_raw(&app, Method::POST, route, Some(&admin), "{").await,
            StatusCode::BAD_REQUEST,
            "bad_request",
        )
        .await;
        assert_error(
            send_raw(&app, Method::POST, route, None, "{").await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;

        let oversized = json!({"padding": "x".repeat(65_537)}).to_string();
        assert_error(
            send_raw(&app, Method::POST, route, Some(&admin), oversized.clone()).await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        )
        .await;
        assert_error(
            send_raw(&app, Method::POST, route, None, oversized).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
}

#[tokio::test]
async fn class_type_and_settings_happy_paths_are_audited_and_partial_updates_merge() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;

    let class = send(
        &app,
        Method::POST,
        "/api/cmdb/classes",
        Some(&admin),
        Some(json!({
            "key":"lab",
            "label":"Lab Equipment",
            "description":"Custom equipment",
            "enabled":true
        })),
    )
    .await;
    assert_eq!(class.status(), StatusCode::OK);
    assert_eq!(json_body(class).await["key"], "lab");

    let ty = send(
        &app,
        Method::POST,
        "/api/cmdb/types",
        Some(&admin),
        Some(json!({
            "key":"scope",
            "class_key":"lab",
            "label":"Oscilloscope",
            "enabled":true
        })),
    )
    .await;
    assert_eq!(ty.status(), StatusCode::OK);
    assert_eq!(json_body(ty).await["class_key"], "lab");

    let patched = send(
        &app,
        Method::PATCH,
        "/api/cmdb/types/scope",
        Some(&admin),
        Some(json!({"label":"Bench Oscilloscope","enabled":false})),
    )
    .await;
    assert_eq!(patched.status(), StatusCode::OK);
    assert_eq!(json_body(patched).await["label"], "Bench Oscilloscope");
    let cleared = send(
        &app,
        Method::PATCH,
        "/api/cmdb/classes/lab",
        Some(&admin),
        Some(json!({"description":null})),
    )
    .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    assert!(json_body(cleared).await["description"].is_null());
    assert_error(
        send(
            &app,
            Method::POST,
            "/api/cmdb/types",
            Some(&admin),
            Some(json!({"key":"ghost","class_key":"missing","label":"Ghost"})),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;

    let settings = send(&app, Method::GET, "/api/cmdb/settings", Some(&admin), None).await;
    assert_eq!(settings.status(), StatusCode::OK);
    let settings = json_body(settings).await;
    assert_eq!(settings["prefix"], "VT");
    let updated = send(
        &app,
        Method::PATCH,
        "/api/cmdb/settings",
        Some(&admin),
        Some(json!({
            "prefix":"LAB",
            "separator":".",
            "number_width":5,
            "discovery_policy":"review_first"
        })),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = json_body(updated).await;
    assert_eq!(updated["prefix"], "LAB");
    assert_eq!(updated["separator"], ".");
    assert_eq!(updated["template"], settings["template"]);
    assert_eq!(updated["starting_number"], settings["starting_number"]);
    assert_eq!(updated["counter_scope"], settings["counter_scope"]);
    let partial = send(
        &app,
        Method::PATCH,
        "/api/cmdb/settings",
        Some(&admin),
        Some(json!({"discovery_policy":"automatic"})),
    )
    .await;
    assert_eq!(partial.status(), StatusCode::OK);
    let partial = json_body(partial).await;
    assert_eq!(partial["prefix"], "LAB");
    assert_eq!(partial["discovery_policy"], "automatic");
    assert_error(
        send(
            &app,
            Method::PATCH,
            "/api/cmdb/settings",
            Some(&admin),
            Some(json!({"template":"{prefix}-{unknown}-{number}"})),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    )
    .await;

    assert_eq!(
        send(
            &app,
            Method::DELETE,
            "/api/cmdb/types/scope",
            Some(&admin),
            None,
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            "/api/cmdb/classes/lab",
            Some(&admin),
            None,
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action LIKE 'cmdb.catalog.%' OR action = 'cmdb.settings.update'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(audits, 8);
}

#[tokio::test]
async fn location_crud_and_conflicts_use_stable_public_envelopes() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;
    let created = send(
        &app,
        Method::POST,
        "/api/cmdb/locations",
        Some(&admin),
        Some(json!({"name":"Home","description":"Main site"})),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let root = json_body(created).await;
    let root_id = root["id"].as_str().unwrap();
    assert_eq!(root["path"], "Home");

    assert_error(
        send(
            &app,
            Method::POST,
            "/api/cmdb/locations",
            Some(&admin),
            Some(json!({"name":"Home"})),
        )
        .await,
        StatusCode::CONFLICT,
        "conflict",
    )
    .await;

    let child = send(
        &app,
        Method::POST,
        "/api/cmdb/locations",
        Some(&admin),
        Some(json!({"parent_id":root_id,"name":"Office"})),
    )
    .await;
    assert_eq!(child.status(), StatusCode::OK);
    let child = json_body(child).await;
    let child_id = child["id"].as_str().unwrap();
    assert_eq!(child["path"], "Home / Office");

    let updated = send(
        &app,
        Method::PATCH,
        &format!("/api/cmdb/locations/{child_id}"),
        Some(&admin),
        Some(json!({"parent_id":root_id,"name":"Studio","description":"Desk"})),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(json_body(updated).await["path"], "Home / Studio");
    let cleared = send(
        &app,
        Method::PATCH,
        &format!("/api/cmdb/locations/{child_id}"),
        Some(&admin),
        Some(json!({"description":null})),
    )
    .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    assert!(json_body(cleared).await["description"].is_null());

    assert_error(
        send(
            &app,
            Method::DELETE,
            &format!("/api/cmdb/locations/{root_id}"),
            Some(&admin),
            None,
        )
        .await,
        StatusCode::CONFLICT,
        "conflict",
    )
    .await;
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/cmdb/locations/{child_id}"),
            Some(&admin),
            None,
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_error(
        send(
            &app,
            Method::PATCH,
            "/api/cmdb/locations/missing",
            Some(&admin),
            Some(json!({"name":"Missing"})),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
}

#[tokio::test]
async fn relationships_resolve_public_selectors_retain_ended_rows_and_index_both_histories() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;
    let operator = session(&db, "operator").await;
    let source = asset(&db, "hw", "hdd", "Disk").await;
    let destination = asset(&db, "sys", "host", "Host").await;
    let old_source_id = source.asset_id.clone();
    let renamed = assets::rename(
        &db,
        &source.resource_id,
        "LAB-DISK-42",
        source.revision,
        context(),
    )
    .await
    .unwrap();

    let created = send(
        &app,
        Method::POST,
        &format!("/api/cmdb/assets/{old_source_id}/relationships"),
        Some(&admin),
        Some(json!({
            "destination_selector": destination.asset_id,
            "type_key":"installed_in",
            "metadata":{"bay":"B2"}
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let relationship = json_body(created).await;
    let relationship_id = relationship["id"].as_str().unwrap();
    assert_eq!(relationship["source_resource_id"], renamed.resource_id);
    assert_eq!(
        relationship["destination_resource_id"],
        destination.resource_id
    );
    assert_eq!(relationship["metadata"]["bay"], "B2");

    let listed = send(
        &app,
        Method::GET,
        &format!("/api/cmdb/assets/{old_source_id}/relationships?limit=20&offset=0"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        json_body(listed).await["relationships"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let ended = send(
        &app,
        Method::DELETE,
        &format!("/api/cmdb/relationships/{relationship_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(ended.status(), StatusCode::OK);
    assert_eq!(json_body(ended).await["active"], false);
    let persisted: (bool, Option<i64>) =
        sqlx::query_as("SELECT active, ended_at FROM cmdb_relationships WHERE id = ?")
            .bind(relationship_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(!persisted.0);
    assert!(persisted.1.is_some());

    for selector in [&old_source_id, &destination.asset_id] {
        let history = send(
            &app,
            Method::GET,
            &format!("/api/cmdb/assets/{selector}/history?limit=20&offset=0"),
            Some(&operator),
            None,
        )
        .await;
        assert_eq!(history.status(), StatusCode::OK);
        let history = json_body(history).await;
        let types: Vec<&str> = history["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|event| event["event_type"].as_str())
            .collect();
        assert!(types.contains(&"cmdb.asset.relationship_started.v1"));
        assert!(types.contains(&"cmdb.asset.relationship_ended.v1"));
    }
}

#[tokio::test]
async fn discovery_ignore_link_and_register_require_current_fingerprints_and_public_selectors() {
    let (db, app) = setup().await;
    let admin = session(&db, "admin").await;
    let host = asset(&db, "sys", "host", "Source Host").await;
    let target = asset(&db, "hw", "hdd", "Existing Disk").await;
    let old_target_id = target.asset_id.clone();
    assets::rename(
        &db,
        &target.resource_id,
        "EXISTING-DISK",
        target.revision,
        context(),
    )
    .await
    .unwrap();
    let ignored = insert_discovery(&db, &host.resource_id, "ignore-fp", "ignore-disk").await;
    let linked = insert_discovery(&db, &host.resource_id, "link-fp", "link-disk").await;
    let registered = insert_discovery(&db, &host.resource_id, "register-fp", "register-disk").await;

    assert_error(
        send(
            &app,
            Method::POST,
            &format!("/api/cmdb/discoveries/{}/ignore", ignored.id),
            Some(&admin),
            Some(json!({"expected_fingerprint":"stale","notes":"old view"})),
        )
        .await,
        StatusCode::CONFLICT,
        "conflict",
    )
    .await;
    let ignored_response = send(
        &app,
        Method::POST,
        &format!("/api/cmdb/discoveries/{}/ignore", ignored.id),
        Some(&admin),
        Some(json!({"expected_fingerprint":ignored.fingerprint,"notes":"not ours"})),
    )
    .await;
    assert_eq!(ignored_response.status(), StatusCode::OK);
    assert_eq!(json_body(ignored_response).await["state"], "ignored");

    let linked_response = send(
        &app,
        Method::POST,
        &format!("/api/cmdb/discoveries/{}/link", linked.id),
        Some(&admin),
        Some(json!({
            "expected_fingerprint":linked.fingerprint,
            "asset_selector":old_target_id,
            "notes":"same device"
        })),
    )
    .await;
    assert_eq!(linked_response.status(), StatusCode::OK);
    assert_eq!(
        json_body(linked_response).await["resource_id"],
        target.resource_id
    );

    let registered_response = send(
        &app,
        Method::POST,
        &format!("/api/cmdb/discoveries/{}/register", registered.id),
        Some(&admin),
        Some(json!({
            "expected_fingerprint":registered.fingerprint,
            "name":"Reviewed Disk","class_key":"hw","type_key":"hdd",
            "notes":"approved"
        })),
    )
    .await;
    let registered_status = registered_response.status();
    let registered_body = json_body(registered_response).await;
    assert_eq!(registered_status, StatusCode::OK, "{registered_body}");
    assert!(registered_body["resource_id"].is_string());

    let list = send(
        &app,
        Method::GET,
        "/api/cmdb/discoveries?limit=20&offset=0",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        json_body(list).await["discoveries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn observation_and_history_reads_resolve_retained_aliases_and_return_bounded_public_records()
{
    let (db, app) = setup().await;
    let operator = session(&db, "operator").await;
    let record = asset(&db, "hw", "hdd", "Observed Disk").await;
    let retained_alias = record.asset_id.clone();
    assets::rename(
        &db,
        &record.resource_id,
        "OBSERVED-DISK",
        record.revision,
        context(),
    )
    .await
    .unwrap();
    let source = asset(&db, "sys", "host", "Observation Source").await;
    let discovery = insert_discovery(&db, &source.resource_id, "observation-fp", "observed").await;
    sqlx::query("UPDATE cmdb_observations SET resource_id = ?, state = 'online' WHERE id = ?")
        .bind(&record.resource_id)
        .bind(&discovery.id)
        .execute(&db)
        .await
        .unwrap();

    let observations = send(
        &app,
        Method::GET,
        &format!("/api/cmdb/assets/{retained_alias}/observations?limit=1&offset=0"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(observations.status(), StatusCode::OK);
    let observations = json_body(observations).await;
    assert_eq!(observations["resource_id"], record.resource_id);
    assert_eq!(observations["observations"].as_array().unwrap().len(), 1);
    assert_eq!(
        observations["observations"][0]["attributes"]["model"],
        "Review Disk"
    );
    assert!(observations["observations"][0]
        .get("attributes_json")
        .is_none());

    let history = send(
        &app,
        Method::GET,
        &format!("/api/cmdb/assets/{retained_alias}/history?limit=1&offset=0"),
        Some(&operator),
        None,
    )
    .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history = json_body(history).await;
    assert_eq!(history["resource_id"], record.resource_id);
    assert_eq!(history["events"].as_array().unwrap().len(), 1);

    assert_error(
        send(
            &app,
            Method::GET,
            "/api/cmdb/assets/not-found/observations?limit=1&offset=0",
            Some(&operator),
            None,
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
}
