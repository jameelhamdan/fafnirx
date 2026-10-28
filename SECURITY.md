# Security policy

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private reporting: **Security → Report a vulnerability** on
<https://github.com/jameelhamdan/fafnirx>. Include what you found, how to reproduce it, and the version
(`fafnirx --version`). This is a one-person project; expect an answer within a few days.

## Scope

fafnirx is meant for a home network, a VPN, or the internet behind a TLS proxy. The master speaks plain HTTP on purpose. The
threat model is in [docs/architecture.md](docs/architecture.md#security-model), the hardening checklist in
[docs/deployment.md](docs/deployment.md#security-checklist). By design:

- A device token adds data to its own device and cannot read, list or delete it. It can read and write the shared folders of
  its user, and those another user shared with them (replaced files stay as versions). One installation holds a device at a
  time, so a copied token cannot run beside the original (the owner is alerted). A leaked token still needs revoking or
  rotating.
- A user's login protects everything else. Roles: administrator, normal, read only. Users see only their own devices and the
  master files; administrators see everything. SMB/NFS shares of `files/` have their own logins and are outside these roles.
- Ids (UUIDs) are not secrets: access is checked on every request.
- Data is not encrypted at rest or on mirrors.
- Client updates need a valid Ed25519 signature from a key that is not on the master, or come from GitHub releases with a checksum.

Reports about these designed limits are welcome as ideas, not vulnerabilities. Ways around them are vulnerabilities.

## Supported versions

Only the latest release gets fixes.
