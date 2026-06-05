use crate::AppState;
use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, patch, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert("x-frame-options",        HeaderValue::from_static("DENY"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("referrer-policy",        HeaderValue::from_static("strict-origin-when-cross-origin"));
    h.insert("content-security-policy", HeaderValue::from_static(
        "default-src 'self'; \
         script-src 'self' 'unsafe-inline' 'unsafe-eval'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         connect-src 'self' ws: wss:; \
         font-src 'self' data:; \
         worker-src blob:; \
         frame-src *; \
         frame-ancestors 'none';"
    ));
    res
}

pub mod alerts;
pub mod apps;
pub mod audit;
pub mod auth;
pub mod automation;
pub mod backups;
pub mod capabilities;
pub mod containers;
pub mod diagnostics;
pub mod files;
pub mod firewall;
pub mod metrics;
pub mod proxy;
pub mod secrets;
pub mod timeline;
pub mod security;
pub mod services;
pub mod status;
pub mod terminal;
pub mod network;
pub mod users;
pub mod settings;
pub mod wireguard;
pub mod vms;
pub mod tags;
pub mod ai;
pub mod bearer_auth;
pub mod integrations;
pub mod models;
pub mod storage;
pub mod system;
pub mod updates;
pub mod totp;
pub mod mods;
pub mod webhooks;

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any);

    Router::new()
        // Health
        .route("/api/health", get(auth::health))
        // Auth
        .route("/api/auth/login",     post(auth::login))
        .route("/api/auth/logout",    post(auth::logout))
        .route("/api/auth/me",        get(auth::me))
        .route("/api/auth/bootstrap", post(auth::bootstrap))
        // TOTP
        .route("/api/auth/totp/setup",   post(totp::setup))
        .route("/api/auth/totp/enable",  post(totp::enable))
        .route("/api/auth/totp/disable", post(totp::disable))
        // Metrics
        .route("/api/metrics/current", get(metrics::get_current))
        .route("/api/metrics/ws", get(metrics::ws_handler))
        // Services
        .route("/api/services", get(services::list))
        .route("/api/services/:name", get(services::get))
        .route("/api/services/:name/action", post(services::action))
        .route("/api/services/:name/logs", get(services::logs))
        // Containers
        .route("/api/containers", get(containers::list))
        .route("/api/containers/images", get(containers::images))
        .route("/api/containers/:id/action", post(containers::action))
        .route("/api/containers/:id/logs", get(containers::logs))
        .route("/api/containers/:id/logs/stream", get(containers::logs_ws))
        .route("/api/containers/:id/exec", get(containers::exec_ws))
        .route("/api/containers/:id/compose", get(containers::get_compose))
        .route("/api/containers/:id/compose/propose", post(containers::propose_compose))
        .route("/api/containers/:id/compose/apply", post(containers::apply_compose))
        // App Vault
        .route("/api/apps/catalog",    get(apps::catalog))
        .route("/api/apps/detect-env", get(apps::detect_env))
        .route("/api/apps/deployed",   get(apps::deployed))
        .route("/api/apps/deploy", post(apps::deploy))
        .route("/api/apps/deploy-custom", post(apps::deploy_custom))
        .route("/api/apps/:project_name", delete(apps::remove_app))
        .route("/api/apps/:project_name/start",   post(apps::start_app))
        .route("/api/apps/:project_name/stop",    post(apps::stop_app))
        .route("/api/apps/:project_name/restart",  post(apps::restart_app))
        .route("/api/apps/:project_name/redeploy", post(apps::redeploy_app))
        .route("/api/apps/:project_name/logs",     get(apps::app_logs))
        .route("/api/apps/:project_name/status",  get(apps::app_status))
        .route("/api/apps/:project_name/compose", get(apps::get_compose).post(apps::update_compose))
        // Backups
        .route("/api/backups", get(backups::list).post(backups::create))
        .route("/api/backups/:id/run", post(backups::run_now))
        .route("/api/backups/:id/check", post(backups::check))
        .route("/api/backups/:id/restore-test", post(backups::restore_test))
        .route("/api/backups/:id", delete(backups::delete))
        // Public status page (no auth)
        .route("/status", get(status::public_page))
        // Status checks
        .route("/api/status-checks", get(status::list).post(status::create))
        .route("/api/status-checks/:id", delete(status::delete))
        // Alerts
        .route("/api/alerts", get(alerts::list))
        .route("/api/alerts/:id/acknowledge", post(alerts::acknowledge))
        .route("/api/alerts/:id/resolve", post(alerts::resolve))
        .route("/api/alerts/:id", delete(alerts::delete_alert))
        // Files
        .route("/api/files/roots",  get(files::roots))
        .route("/api/files/list",   get(files::list))
        .route("/api/files/read",   get(files::read_file))
        .route("/api/files/write",  post(files::write_file))
        .route("/api/files/mkdir",  post(files::mkdir))
        .route("/api/files/delete", delete(files::delete))
        .route("/api/files/rename",   post(files::rename))
        .route("/api/files/activity", get(files::activity))
        .route("/api/files/raw",      get(files::serve_raw))
        // AI / llama.cpp
        .route("/api/ai/llama",        get(ai::llama_status))
        .route("/api/ai/llama/unload", post(ai::llama_unload))
        // Models
        .route("/api/models",              get(models::list_models))
        .route("/api/models/download",     post(models::start_download))
        .route("/api/models/download/:id", get(models::download_status))
        .route("/api/models/active",          get(models::get_active))
        .route("/api/models/load",            post(models::load_model))
        .route("/api/models/ollama",            get(models::get_ollama_tags))
        .route("/api/models/ollama/pull",       post(models::start_ollama_pull))
        .route("/api/models/ollama/pull/:id",   get(models::get_ollama_pull_status))
        .route("/api/models/ollama/create",     post(models::start_ollama_create))
        .route("/api/models/ollama/create/:id", get(models::get_ollama_create_status))
        .route("/api/models/:filename",       delete(models::delete_model))
        // Proxy manager
        .route("/api/proxy", get(proxy::list).post(proxy::create))
        .route("/api/proxy/nginx-install-cmd", get(proxy::nginx_install_cmd))
        .route("/api/proxy/nginx-setup", get(proxy::nginx_setup_status))
        .route("/api/proxy/nginx/action", post(proxy::nginx_action))
        .route("/api/proxy/nginx/logs", get(proxy::nginx_logs))
        .route("/api/proxy/nginx/status", get(proxy::nginx_status))
        .route("/api/proxy/ai-auto", post(proxy::ai_auto_proxy))
        .route("/api/proxy/:id", delete(proxy::delete_proxy))
        .route("/api/proxy/:id/toggle", post(proxy::toggle))
        // Security
        .route("/api/security/sessions", get(security::list_sessions))
        .route("/api/security/sessions/revoke-others", post(security::revoke_all_other))
        .route("/api/security/sessions/:id", delete(security::revoke_session))
        // Users
        .route("/api/users", get(users::list).post(users::create))
        .route("/api/users/me/password", post(users::change_my_password))
        .route("/api/users/:id", delete(users::delete_user))
        // Terminal
        .route("/api/terminal/ws", get(terminal::ws_handler))
        .route("/api/terminal/ssh/sessions", get(terminal::list_ssh_sessions).post(terminal::create_ssh_session))
        .route("/api/terminal/ssh/sessions/:id", delete(terminal::delete_ssh_session))
        .route("/api/terminal/ssh/ws", get(terminal::ssh_ws_handler))
        // Audit
        .route("/api/audit", get(audit::list))
        // Timeline
        .route("/api/timeline", get(timeline::list))
        // Automation
        .route("/api/automation", get(automation::list).post(automation::create))
        .route("/api/automation/:id", delete(automation::delete).patch(automation::update))
        .route("/api/automation/:id/run", post(automation::run_now))
        .route("/api/automation/:id/runs", get(automation::runs))
        // Firewall
        .route("/api/firewall", get(firewall::get_status))
        .route("/api/firewall/rules", post(firewall::add_rule))
        .route("/api/firewall/rules/delete", post(firewall::delete_rule))
        .route("/api/firewall/action", post(firewall::firewall_action))
        // Capabilities
        .route("/api/capabilities", get(capabilities::get_capabilities))
        // Diagnostics
        .route("/api/diagnostics", get(diagnostics::get_diagnostics))
        // Secrets
        .route("/api/secrets", get(secrets::list).post(secrets::create))
        .route("/api/secrets/:id", delete(secrets::delete).patch(secrets::update))
        .route("/api/secrets/:id/reveal", get(secrets::reveal))
        // Network neighbors (LAN scan)
        .route("/api/network/neighbors", get(network::neighbors))
        // WireGuard
        .route("/api/wireguard", get(wireguard::list))
        .route("/api/wireguard/peers", post(wireguard::add_peer))
        .route("/api/wireguard/peers/:id", delete(wireguard::delete_peer))
        .route("/api/settings/ai-url", get(settings::get_ai_url).post(settings::set_ai_url))
        .route("/api/settings/general", get(settings::get_general).post(settings::set_general))
        .route("/api/settings/notifications", get(settings::get_notifications).post(settings::set_notifications))
        .route("/api/settings/notifications/test", post(settings::test_notification))
        // VMs (local KVM + Proxmox)
        .route("/api/vms/local", get(vms::list_local))
        .route("/api/vms/local/action", post(vms::local_action))
        .route("/api/vms/proxmox/config", get(vms::get_proxmox_config).post(vms::set_proxmox_config))
        .route("/api/vms/proxmox/vms", get(vms::list_proxmox))
        .route("/api/vms/proxmox/action", post(vms::proxmox_action))
        .route("/api/vms/proxmox/test", post(vms::test_proxmox))
        // Tags
        .route("/api/tags",           get(tags::list).post(tags::create))
        .route("/api/tags/for",       get(tags::tags_for_resource))
        .route("/api/tags/map",       get(tags::tags_map))
        .route("/api/tags/assign",    post(tags::assign))
        .route("/api/tags/unassign",  post(tags::unassign))
        .route("/api/tags/:id",       delete(tags::delete).patch(tags::update))
        // Storage management
        .route("/api/storage/devices",      get(storage::list_devices))
        .route("/api/storage/mounts",       get(storage::list_mounts_handler))
        .route("/api/storage/mount",        post(storage::mount_device))
        .route("/api/storage/umount",       post(storage::umount_device))
        .route("/api/storage/fstab",        get(storage::get_fstab).post(storage::add_fstab))
        .route("/api/storage/fstab/:idx",   delete(storage::remove_fstab))
        .route("/api/storage/smart/:dev",   get(storage::get_smart))
        .route("/api/storage/raid",         get(storage::get_raid))
        .route("/api/storage/raid/create",  post(storage::create_raid))
        .route("/api/storage/raid/stop",    post(storage::stop_raid))
        .route("/api/storage/format",       post(storage::format_device))
        .route("/api/storage/paths",        get(storage::get_storage_paths).post(storage::set_storage_paths))
        // Integrations (API tokens, Odysseus config, manifest, SSE, webhooks)
        .route("/api/integrations/scopes",                  get(integrations::scopes_list))
        .route("/api/integrations/tokens",                  get(integrations::list_tokens).post(integrations::create_token))
        .route("/api/integrations/tokens/:id",              delete(integrations::revoke_token))
        .route("/api/integrations/odysseus/config",         get(integrations::get_config).post(integrations::save_config))
        .route("/api/integrations/odysseus/manifest",       get(integrations::manifest))
        .route("/api/integrations/events",                  get(integrations::event_stream))
        .route("/api/integrations/webhooks",                post(integrations::webhook))
        .route("/api/integrations/actions",                 get(integrations::recent_actions))
        .route("/api/system/version",       get(system::version))
        .route("/api/system/update-check",  get(system::update_check))
        .route("/api/system/restart",       post(system::restart))
        .route("/api/system/update",        post(system::update))
        // Updates page
        .route("/api/updates/voidtower",           get(updates::vt_info))
        .route("/api/updates/voidtower/check",     post(updates::check_vt))
        .route("/api/updates/voidtower/apply",     post(updates::apply_vt))
        .route("/api/updates/voidtower/rollback",  post(updates::rollback_vt))
        .route("/api/updates/odysseus",            get(updates::odysseus_info))
        .route("/api/updates/odysseus/apply",      post(updates::apply_odysseus))
        .route("/api/updates/docker",              get(updates::docker_info))
        .route("/api/updates/docker/check",        post(updates::docker_check))
        .route("/api/updates/docker/:id/apply",    post(updates::docker_apply))
        .route("/api/updates/os",                  get(updates::os_info))
        .route("/api/updates/os/apply",            post(updates::apply_os))
        // Mods
        .route("/api/mods",              get(mods::get_status))
        .route("/api/mods/config",       post(mods::save_config))
        .route("/api/mods/fetch",        post(mods::fetch_mod))
        .route("/api/mods/diff",         get(mods::get_diff))
        .route("/api/mods/apply",        post(mods::apply_mod))
        .route("/api/mods/rollback",     post(mods::rollback_mod))
        // Notification webhooks
        .route("/api/webhooks",           get(webhooks::list).post(webhooks::create))
        .route("/api/webhooks/:id",       patch(webhooks::update).delete(webhooks::delete))
        .route("/api/webhooks/:id/test",  post(webhooks::test_webhook))
        .layer(cors)
        .layer(middleware::from_fn(security_headers))
        .layer(axum::middleware::from_fn_with_state(state.clone(), bearer_auth::middleware))
        .with_state(state)
}
