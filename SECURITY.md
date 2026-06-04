# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| main    | Yes       |

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues.

Use GitHub's private vulnerability reporting feature, or contact the maintainers
directly. Include description, reproduction steps, impact, and any suggested fix.

You will receive a response within 72 hours. Critical issues are patched and
released as soon as possible.

## Security Model

- VoidTower is designed for use on trusted private networks (LAN, VPN).
- Do not expose VoidTower directly to the public internet without TLS and a
  reverse proxy.
- The daemon requires significant system privileges. Treat it accordingly.
- All dangerous actions are logged to the audit trail.
- AI-triggered actions (Odysseus/MCP) require explicit scope grants, risk
  classification, and confirmation for high-risk operations.
- Secrets are stored encrypted at rest. Bootstrap tokens are mode 0600.
- `/etc/voidtower` is mode 0700.
- Sessions use secure, httponly, sameSite cookies.
- Login rate limiting is applied per IP.
