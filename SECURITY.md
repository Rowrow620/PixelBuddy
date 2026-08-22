# Security Policy

## Supported version

PixelBuddy is pre-release software. Security fixes are applied to the latest `main` branch; older snapshots and unofficial builds are not supported.

## Reporting a vulnerability

Use GitHub's **Security** tab and **Report a vulnerability** to send details privately. If private vulnerability reporting is unavailable, open a minimal public issue asking for a private contact channel and do not include exploit details, private project files, or personal data.

Include the affected build or commit, platform (native or browser), reproduction steps, impact, and a small sanitized sample when one is required. Project and image files are treated as untrusted input.

## Dependency policy

Pull requests that change Cargo inputs run `cargo-deny` license, source, and RustSec checks. A scheduled run refreshes the advisory database weekly. The sole accepted advisory is documented in `docs/SECURITY_AND_RECOVERY.md`; new vulnerabilities, unsoundness reports, yanks, unknown sources, and unapproved licenses fail the policy gate.