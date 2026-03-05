# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.2.x   | ✅ Current          |
| < 0.2   | ❌ No longer supported |

## Reporting a Vulnerability

If you discover a security vulnerability in HIEF, please report it responsibly.

**⚠️ Do NOT open a public GitHub issue for security vulnerabilities.**

### How to Report

1. **Email**: Send a detailed report to the maintainers via GitHub's
   [private vulnerability reporting](https://github.com/hiranp/hief/security/advisories/new)
2. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if you have one)

### What to Expect

- **Acknowledgment** within 48 hours
- **Assessment** within 1 week
- **Fix or mitigation** as soon as possible, depending on severity
- **Credit** in the release notes (unless you prefer anonymity)

## Security Considerations

HIEF is a **local-first** tool. By design:

- **No network access** — HIEF does not send data to external services
  (unless the optional `embeddings` feature is enabled for vector search)
- **Local database** — all data is stored in `.hief/hief.db` on your filesystem
- **No authentication** — the MCP server binds to stdio by default; HTTP
  transport binds to localhost only
- **No code execution** — HIEF indexes and searches code but never executes it

### If Using HTTP Transport

When running `hief serve --transport http`, the server binds to `localhost`
by default. If you expose it on a network interface, be aware that the MCP
server has **no built-in authentication or authorization**. Use a reverse
proxy with authentication if you need network access.

### Dependency Security

We monitor dependencies for known vulnerabilities. If you notice a vulnerable
dependency, please report it using the process above.

## Security Best Practices for Users

- Keep your HIEF installation up to date
- Do not expose the HTTP transport to untrusted networks
- Review `.hief/conventions.toml` for project-specific security rules
- Use `hief eval run` to check for anti-patterns like bare `.unwrap()` calls
