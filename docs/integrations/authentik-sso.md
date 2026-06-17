# Authentik SSO — central identity for VoidTower and App Vault apps

VoidTower can use [Authentik](https://goauthentik.io) as a platform-wide identity
provider: a "Login with Authentik" option (with MFA enforced by Authentik's own
login flow) for VoidTower itself, plus an opt-in toggle to put any App Vault proxy
behind the same login before traffic reaches the app.

Local username/password (+ optional TOTP) login is never removed — Authentik SSO is
additive, so there's always a local admin fallback.

## 1. Deploy Authentik

Deploy `authentik` from the App Vault. It joins the `vt-proxy` Docker network under
the alias `authentik` (port `9000` internally) — this is the address VoidTower's
forward-auth gate and OIDC client both assume by default. Log in to its own UI
(`admin@authentik.local` / the generated `AUTHENTIK_BOOTSTRAP_PASSWORD`) to finish
its first-run setup.

## 2. VoidTower SSO (OIDC login)

1. In Authentik, create an **OAuth2/OpenID Provider** and an **Application** for
   VoidTower. Set the redirect URI to:
   ```
   https://<your-voidtower-host>/api/auth/oidc/callback
   ```
2. In VoidTower, go to **Settings → Authentik SSO** and fill in:
   - **Issuer URL** — the provider's OIDC issuer (Authentik shows this on the
     provider page, e.g. `https://authentik.local/application/o/voidtower/`).
   - **Client ID** / **Client secret** — from the Authentik application.
   - **Redirect URL** — must match the URI registered in step 1 exactly.
   - **Scopes** / **Role claim** — defaults (`openid profile email groups` /
     `groups`) work with Authentik's default scope mappings.
   - **Role mapping** — map Authentik group names to VoidTower roles
     (`owner` / `admin` / `operator` / `viewer`). Users not in any mapped group get
     the **Default role**. Role is re-evaluated on every Authentik login, so group
     changes in Authentik take effect immediately.
   - **Auto-create users on first Authentik login** — leave on unless you want to
     pre-create every account manually.
3. Save (review the change plan, then confirm). The Login page now shows a
   "Login with Authentik" button under the local form.

Accounts provisioned this way (`auth_source = oidc`) get an unusable local password
hash — they can only sign in via Authentik unless an admin later sets a real
password for them.

## 3. Protecting an App Vault app with Authentik

This uses Authentik's **embedded outpost** (already part of the `authentik-server`
container — no second container to deploy) in forward-auth mode:

1. In Authentik, create a **Proxy Provider** in *forward auth (single application)*
   mode for the app you want to protect, plus an **Application** using it.
2. In VoidTower, go to **Proxies**, create or edit the proxy rule for that app, and
   check **"Protect with Authentik."** This checkbox is disabled until SSO is
   configured (step 2 above).
3. Save. The generated nginx conf now sends every request through
   `http://authentik:9000/outpost.goauthentik.io/auth/nginx` before reaching the
   app's `proxy_pass` upstream — visitors must authenticate (and pass MFA, if
   required by the Authentik provider) before the app ever sees the request.

This toggle is **off by default** for every proxy — some apps (e.g. Vaultwarden)
already have their own strong auth/MFA, and double-gating them may be redundant or
interfere with their own login flow. Turn it on per app as needed.

## Notes

- The forward-auth gate and the OIDC login are independent — you can use either one
  without the other (e.g. SSO login for VoidTower with no app-vault apps gated, or
  vice versa).
- If you run Authentik somewhere other than via the App Vault catalog entry (a
  different hostname/port, or an external instance), the forward-auth gate's
  hardcoded `http://authentik:9000` upstream won't resolve — this is a known
  limitation of the current implementation, which assumes the in-vault deployment.
- See [`docs/NETWORKING.md`](../NETWORKING.md) for how the nginx-proxy container
  reaches host vs. container services in general.
