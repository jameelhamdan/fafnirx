# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem. Use GitHub's private reporting:
**Security → Report a vulnerability** on <https://github.com/jameelhamdan/fafnirx>. Include what you found, how to
reproduce it, and the version (`fafnirx --version`). You will get an answer within a few days; this is a
one-person project, so please be patient.

## Scope

fafnirx is meant for a home network or a VPN, or for the internet behind a TLS proxy such as a Cloudflare tunnel.
The master speaks plain HTTP on purpose. The threat model is in
[docs/architecture.md](docs/architecture.md#security-model) and the hardening checklist in
[docs/deployment.md](docs/deployment.md#security-checklist). In particular:

- A device token can only add data; it cannot read, list or delete. A device is used by one installation of the client at a time (the first to connect holds it for as long as it is heard from), so a copied token cannot run beside the original; the owner is alerted when it tries. This protects the data from two views of one device, not against someone who has the token and waits for the first to go quiet: revoke or rotate a leaked token.
- A user's login protects everything else. There are three roles (administrator, normal, read only); users see only their own devices and the master files, and administrators see everything. SMB or NFS shares of `files/` have logins of their own and are not covered by these roles.
- Data is not encrypted at rest or on mirrors by fafnirx.
- Client updates are accepted only with a valid Ed25519 signature from a key that is not on the master.

Reports about these designed limits are welcome as ideas, but are not vulnerabilities. Reports of ways around them are.

## Supported versions

Only the latest release gets fixes.
