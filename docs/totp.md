# Two-Factor Authentication (TOTP)

VoidTower supports TOTP-based 2FA (RFC 6238 — the same standard used by Google Authenticator, Bitwarden, Authy, and compatible apps). It uses SHA-1, 6-digit codes, and a 30-second period with ±30 second clock-skew tolerance.

2FA is per-user and opt-in. Each user manages their own TOTP enrollment from their account settings.

---

## Enabling TOTP

1. Go to **VoidTower → Settings → Account → Two-Factor Authentication**
2. Click **Set up 2FA**
3. Scan the QR code with your authenticator app (or manually enter the secret shown below it)
4. Enter the 6-digit code from your app to verify — this confirms the secret was imported correctly and enables 2FA on your account
5. Store your secret somewhere safe — there is no recovery code system

Once enabled, you will be prompted for a TOTP code on every login after entering your password.

### Compatible apps

Any RFC 6238 TOTP app works:
- Bitwarden (built-in authenticator)
- Aegis (Android, open source)
- Raivo (iOS)
- 1Password (built-in TOTP)
- Google Authenticator
- Authy

---

## Disabling TOTP

1. Go to **VoidTower → Settings → Account → Two-Factor Authentication**
2. Click **Disable 2FA**
3. Enter your current 6-digit code to confirm — this verifies you still have access to the authenticator before removing it

The TOTP secret is wiped from the database on disable.

---

## Recovery (lost authenticator)

If you have lost access to your authenticator app and cannot log in, an admin or owner can reset your account from the CLI or from another admin session.

**From the CLI (any deployment):**

```bash
# Docker
docker exec voidtower voidtower user reset-password --username <name> --password <newpw>

# Bare metal / LXC
sudo voidtower user reset-password --username <name> --password <newpw>
```

A password reset also clears TOTP — the user will be able to log in with the new password without a 2FA code, and will be required to change their password on first login. They can then re-enroll TOTP from their account settings.

**From the UI (admin/owner only):**

Go to **Settings → Users**, find the affected user, and use **Reset password** — this has the same effect as the CLI command.

---

## API reference

| Endpoint | Description |
|---|---|
| `POST /api/totp/setup` | Generate a new TOTP secret and return `{ secret, uri }` for QR display. The secret is stored but TOTP is not yet active. |
| `POST /api/totp/enable` | Verify `{ code }` against the pending secret and enable TOTP. |
| `POST /api/totp/disable` | Verify `{ code }` then disable TOTP and wipe the secret. |

All three endpoints require a valid session cookie (`vt_session`). No admin role is needed — users manage their own 2FA.
