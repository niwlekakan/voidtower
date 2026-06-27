# API Reference

All endpoints require a valid `vt_session` cookie except `/api/auth/*`, `/api/status`, and `/api/system/version`.

---

## Auth

```
POST /api/auth/bootstrap   { token, username, password }
POST /api/auth/login       { username, password }
POST /api/auth/logout
GET  /api/auth/me
```

## Metrics

```
GET /api/metrics/current
GET /api/metrics/ws        WebSocket (1 s interval)
```

## Services

```
GET  /api/services
POST /api/services/:name/action   { action: start|stop|restart|enable|disable }
GET  /api/services/:name/logs
```

## Containers

```
GET  /api/containers
GET  /api/containers/images
POST /api/containers/:id/action   { action: start|stop|restart|remove }
GET  /api/containers/:id/logs
GET  /api/containers/:id/exec     WebSocket PTY
GET  /api/containers/:id/compose
POST /api/containers/:id/compose/propose   { content }
POST /api/containers/:id/compose/apply     { content }
```

## App Vault

```
GET  /api/apps/catalog
GET  /api/apps/deployed
POST /api/apps/deploy              { app_id, project_name?, env_overrides? }
POST /api/apps/:name/start|stop|restart|redeploy
GET  /api/apps/:name/compose
POST /api/apps/:name/compose       { content }
GET  /api/apps/:name/logs
GET  /api/apps/:name/status
DELETE /api/apps/:name
```

## Models

```
GET  /api/models
POST /api/models/download          { url, filename? }
GET  /api/models/download/:id
GET  /api/models/active
POST /api/models/load              { filename }
DELETE /api/models/:filename
POST /api/models/ollama/pull       { model }
GET  /api/models/ollama/pull/:id
POST /api/models/ollama/create     { filename }
GET  /api/models/ollama/create/:id
```

## AI / GPU

```
GET  /api/ai/llama
POST /api/ai/llama/unload
```

## VMs

```
GET  /api/vms/local
POST /api/vms/local/action         { name, action }
GET  /api/vms/proxmox/config
POST /api/vms/proxmox/config
GET  /api/vms/proxmox/vms
POST /api/vms/proxmox/action       { vmid, kind, node, action }
POST /api/vms/proxmox/test
```

## Files

```
GET  /api/files/roots
GET  /api/files/list?path=
GET  /api/files/read?path=
GET  /api/files/raw?path=
POST /api/files/write              { path, content }
POST /api/files/mkdir              { path }
DELETE /api/files/delete?path=
POST /api/files/rename             { from, to }
```

## Proxy

```
GET  /api/proxy
POST /api/proxy                    { domain, upstream, ssl, allow_embed? }
DELETE /api/proxy/:id
POST /api/proxy/:id/toggle
```

## Firewall

```
GET  /api/firewall
POST /api/firewall/rules           { action, direction, port, protocol, from? }
POST /api/firewall/rules/delete    { rule_number }
POST /api/firewall/action          { action: enable|disable|reload|reset }
```

## WireGuard

```
GET  /api/wireguard
POST /api/wireguard/peers          { name, interface, server_endpoint? }
DELETE /api/wireguard/peers/:id
```

## Storage

```
GET  /api/storage/devices
GET  /api/storage/mounts
POST /api/storage/mount
POST /api/storage/umount
GET  /api/storage/fstab
POST /api/storage/fstab
DELETE /api/storage/fstab/:idx
GET  /api/storage/smart/:dev
GET  /api/storage/raid
POST /api/storage/raid/create
POST /api/storage/raid/stop
POST /api/storage/format
GET  /api/storage/paths
POST /api/storage/paths
```

## Network

```
GET /api/network
GET /api/network/neighbors
```

## Backups

```
GET  /api/backups
POST /api/backups                  { name, source_path, repo_path, password }
POST /api/backups/:id/run
POST /api/backups/:id/check
POST /api/backups/:id/restore-test
DELETE /api/backups/:id
```

## Alerts & status checks

```
GET  /api/alerts?state=&severity=
POST /api/alerts/:id/acknowledge
POST /api/alerts/:id/resolve
DELETE /api/alerts/:id
GET  /api/status-checks
POST /api/status-checks            { name, type, target, interval_secs? }
DELETE /api/status-checks/:id
GET  /status                       Public HTML page (no auth)
```

## Automation

```
GET  /api/automation
POST /api/automation               { name, command, schedule, enabled? }
PATCH /api/automation/:id
DELETE /api/automation/:id
POST /api/automation/:id/run
GET  /api/automation/:id/runs
```

## Secrets

```
GET  /api/secrets
POST /api/secrets                  { name, description, value }
PATCH /api/secrets/:id
DELETE /api/secrets/:id
GET  /api/secrets/:id/reveal
```

## Tags

```
GET  /api/tags
POST /api/tags                     { name, color }
PATCH /api/tags/:id
DELETE /api/tags/:id
GET  /api/tags/map?type=
POST /api/tags/assign              { tag_id, resource_type, resource_id }
POST /api/tags/unassign            { tag_id, resource_type, resource_id }
```

## Timeline

```
GET /api/timeline?limit=&offset=&category=&outcome=&search=
```

## Users

```
GET  /api/users
POST /api/users                    { username, password, role }
DELETE /api/users/:id
POST /api/users/me/password        { password }
```

## Security

```
GET  /api/security/sessions
POST /api/security/sessions/revoke-others
DELETE /api/security/sessions/:id
```

## System

```
GET  /api/system/version
GET  /api/system/update-check
POST /api/system/restart
POST /api/system/update
```

## Integrations

```
GET  /api/integrations/scopes
GET  /api/integrations/tokens
POST /api/integrations/tokens                    { name, scopes[], expires_days? }
DELETE /api/integrations/tokens/:id
GET  /api/integrations/odysseus/config
POST /api/integrations/odysseus/config           { enabled?, mcp_enabled?, allowed_url?, webhook_secret?, emergency_disable? }
GET  /api/integrations/odysseus/manifest
GET  /api/integrations/events                    SSE stream
POST /api/integrations/webhooks                  { automation_id, dry_run? }
GET  /api/integrations/actions
```

## Voidwatch (Odysseus-side)

```
GET  /api/voidwatch/config
POST /api/voidwatch/config         { enabled, base_url, api_token, webhook_secret, auto_action_policy }
POST /api/voidwatch/emergency-disable
POST /api/voidwatch/test
GET  /api/voidwatch/manifest
GET  /api/voidwatch/toolpacks
GET  /api/voidwatch/actions
POST /api/voidwatch/webhook
```

## Capabilities & diagnostics

```
GET /api/capabilities
GET /api/diagnostics
```
