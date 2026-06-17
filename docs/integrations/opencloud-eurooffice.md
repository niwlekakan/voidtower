# Connecting Euro-Office to OpenCloud

Euro-Office (the `eurooffice` App Vault entry) is a headless ONLYOFFICE-compatible
document server — it has no useful UI of its own. To edit documents from OpenCloud,
OpenCloud's bundled **collaboration** service has to be pointed at it via the WOPI
protocol.

Verified against `opencloudeu/opencloud:latest`'s actual config struct (`strings` on
the binary, `services/collaboration/pkg/config`) — these env var names and
descriptions are taken directly from the binary, not guessed.

## 1. Deploy both apps

Deploy `eurooffice` from the App Vault first and grab its generated
`EUROOFFICE_JWT_SECRET` from the deploy modal's "Generated secrets" box (or via the
admin-panel bootstrap flow — see below). Deploy `opencloud` if you haven't already.

## 2. Enable the collaboration service

OpenCloud runs as a single binary that only starts a default subset of services.
`collaboration` is not in that default subset, so it must be added explicitly via
`OC_ADD_RUN_SERVICES` (a comma-separated list — verified env var, see
`OC_EXCLUDE_RUN_SERVICES`/`OC_ADD_RUN_SERVICES` in the binary's config defaults).

## 3. Configure the WOPI bridge

Add these environment variables to the `opencloud` service in
`app-vault/apps/opencloud.yml` (or via the App Vault's compose editor for an
already-deployed instance):

| Variable | Purpose |
|---|---|
| `OC_ADD_RUN_SERVICES=collaboration` | Starts the bundled collaboration (WOPI bridge) service alongside the default services. |
| `COLLABORATION_APP_NAME=EuroOffice` | Name shown to users in the editor UI. |
| `COLLABORATION_APP_PRODUCT=OnlyOffice` | Euro-Office is ONLYOFFICE-API-compatible — tell collaboration to speak that protocol. Valid values: `Collabora`, `OnlyOffice`, `Microsoft365`, `MicrosoftOfficeOnline`. |
| `COLLABORATION_APP_ADDR=http://eurooffice:80` | Where collaboration reaches the Euro-Office container. Use the internal Docker network address (`eurooffice` service alias on `vt-proxy`), not `eurooffice.local`, to avoid an extra proxy hop. |
| `COLLABORATION_APP_INSECURE=true` | Skip TLS verification — both apps talk over plain HTTP internally. |
| `COLLABORATION_APP_PROOF_DISABLE=true` | ONLYOFFICE doesn't implement WOPI proof-key verification; collaboration will reject every request without this. |
| `COLLABORATION_WOPI_SRC=https://opencloud.local` | The externally-reachable base URL where Euro-Office should call *back* into OpenCloud's collaboration service. |

`COLLABORATION_WOPI_SECRET` is generated automatically if unset — it signs the
internal WOPI/REVA token exchange between OpenCloud's own gateway and the
collaboration service. It is **not** the same secret as `EUROOFFICE_JWT_SECRET`.

## 4. Euro-Office's own JWT secret

Euro-Office independently validates every incoming API request against its own
`JWT_SECRET` (`EUROOFFICE_JWT_SECRET` in our catalog). Whether collaboration's
outgoing requests need to be signed with that same secret, or whether ONLYOFFICE-mode
JWT enforcement needs to be relaxed for collaboration to work at all, **wasn't
something I could verify from the binary alone** — this is the one part of this doc
that's a starting point, not a confirmed fact.

If documents fail to load with a 401/JWT-related error in either container's logs
after completing steps 1–3:

```
docker logs vt-eurooffice-eurooffice-1 2>&1 | grep -i jwt
docker logs vt-opencloud-opencloud-1   2>&1 | grep -i -E "wopi|collaboration|401"
```

The Euro-Office admin panel (bootstrap instructions below) can also show or relax the
JWT requirement directly if collaboration's requests keep getting rejected.

## Euro-Office admin panel bootstrap

Euro-Office's admin panel isn't started by default. To access it:

```
docker exec vt-eurooffice-eurooffice-1 supervisorctl start adminpanel
docker exec vt-eurooffice-eurooffice-1 sed -i 's,autostart=false,autostart=true,' /etc/supervisor/conf.d/ds-adminpanel.conf
docker logs vt-eurooffice-eurooffice-1 2>&1 | grep -i bootstrap
```

The bootstrap code printed in the logs is valid for 1 hour and is required for first
access to the admin panel.
