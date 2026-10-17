# Security policy

## Supported versions

Security fixes are released for the latest version only. Run the most recent release
(`ghcr.io/teopost/getpaper:<version>`) or `latest`, and update when a new release is published.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Report vulnerabilities privately through GitHub:
[Report a vulnerability](https://github.com/teopost/getpaper/security/advisories/new)
(the **Security** tab of the repository → **Report a vulnerability**).

Include as much as you can: the affected version, a description of the issue and its impact, steps to reproduce or a
proof of concept, and any suggested fix. You will receive an acknowledgement as soon as possible; once the issue is
confirmed, a fix is prepared and released, and the advisory is published with credit to you unless you prefer
otherwise.

## Scope

GetPaper is self-hosted: the security of an installation also depends on how it is deployed. Before reporting, please
check that the problem is in GetPaper itself and not in the surrounding setup (for example a missing HTTPS reverse
proxy, a weak `POSTGRES_PASSWORD` or a database port exposed to the internet).
