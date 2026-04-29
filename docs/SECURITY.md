# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest (main branch) | ✅ |
| All prior releases | ❌ (self-hosted; update to latest) |

xcalibre-server is self-hosted software with a single active release line. Security fixes ship in patch releases. Older versions do not receive backported patches; operators should update.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report via GitHub private security advisories once the repository is public:
→ https://github.com/j-zuilkowski/xcalibre-server/security/advisories/new

Until the repository is public, report directly to the maintainer by email.

Include: description, steps to reproduce, impact assessment, suggested fix.

## Response SLA

| Severity | Acknowledge | Patch release |
|----------|-------------|---------------|
| Critical | 24 hours    | 7 days        |
| High     | 48 hours    | 14 days       |
| Medium   | 7 days      | 30 days       |
| Low/Info | 14 days     | Next minor    |

## Scope

In scope: auth bypass, privilege escalation, injection (SQL/command/SSRF/prompt), path traversal, sensitive data exposure, cryptographic weaknesses.

Out of scope: physical access, social engineering, attacks requiring existing admin access, unauthenticated DoS, theoretical issues without demonstrated impact.

## Known Design Decisions

The following are intentional and not considered vulnerabilities:

- CSP includes 'unsafe-inline' on script-src and style-src to support epub.js rendering and shadcn/ui dynamic styles.
- LLM endpoint URLs are operator-configured and trusted (not user-supplied). SSRF protection requires allow_private_endpoints = true for local model servers; this is an explicit operator opt-in.
- The OPDS feed is unauthenticated by default. Enable opds.require_auth = true in config.toml to require authentication.

## Dependency Audit

cargo audit is run before every release. Suppressed advisories are documented in .cargo/audit.toml with justification comments.
