# Security Policy

## Reporting a vulnerability

Please do **not** open a public issue for security vulnerabilities
(account takeover, authentication bypass, DoS, SQL/protocol injection, IP-ban
evasion, etc.).

Instead, report privately:

- via GitHub [private vulnerability reporting](../../security/advisories/new) (preferred), or
- through the moderation staff on the [Discord server](https://discord.gg/ZRC4DnP9Z2).

Include a description, reproduction steps and your deployment setup
(Docker/source, versions). You will get an acknowledgement within a few days
and a fix timeline once the issue is confirmed.

## Scope notes

- The backend trusts reverse-proxy headers (`cf-connecting-ip`,
  `x-forwarded-for`) — see the README section *"Running behind Cloudflare"*.
  Deployments must block direct origin access; reports relying on a public
  instance that exposes port 3000 to the internet will be treated as
  deployment misconfiguration, not a code vulnerability.
- The bot-defense fingerprint collector is explicitly documented in the
  README (what it collects, what is stored). Reports about "the backend
  fingerprints clients" are not vulnerabilities — that is a stated feature.

## Supported versions

Only the latest tagged release receives security fixes.
