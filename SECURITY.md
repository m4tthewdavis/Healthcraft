# Security policy

HealthCraft is built to hold health information, so security reports are welcome and taken
seriously, even at this pre-alpha stage.

## Reporting a vulnerability

Please report privately through GitHub's **Report a vulnerability** (Security › Advisories) on this
repository. Do not open a public issue containing an exploit, a crash file, credentials or any
patient data. We will acknowledge the report, agree a fix and a disclosure date with you, and
credit you if you wish.

Include the version or commit, the affected component, the impact, and exact reproduction steps.

## Test data

- **Never send or attach real patient data**, even "de-identified". Reproduce with synthetic data
  (the files in `fixtures/`, or Synthea output).
- Crash-inducing inputs accepted into the repository must be minimized, synthetic, and come with a
  regression test.

## Scope

- Parsers: FHIR bundles (`crates/fhir`), HL7 v2 (`crates/hl7v2`), terminology files.
- Authorization: the role policy in `crates/engine` and any path that bypasses `Session::dispatch`.
- The access log (`crates/audit`): anything that lets an access go unrecorded or lets entries be
  altered undetected.
- De-identification (`crates/research/src/deid.rs`): any identifier that survives.
- The HTTP API (`crates/api`): authentication, request limits, error responses that leak data.
- Dependencies, CI and release artifacts.

## Current limitations

These are known and on the roadmap; reports about them are still useful if you find a concrete
exploit:

- Storage is in memory only; there is no encryption at rest yet.
- The HTTP API uses static bearer tokens per role, compares them in constant time, and serves plain
  HTTP on loopback by default. SMART on FHIR / OAuth 2.0 and TLS are not implemented.
- Roles are coarse: there is no per-patient consent, care-team scoping or break-the-glass yet.
- The access log is hash-chained but held in memory; it detects tampering, not truncation of its
  newest entries.
