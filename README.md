<h1 align="center">HealthCraft</h1>

<p align="center">
  <b>An open-source, research-native electronic health record, written entirely in Rust.</b><br>
  Every clinical fact is an event. The chart, the FHIR API and the research warehouse are all
  live views of the same record, so research data is current the moment care is documented.
</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-3a3a3a?style=flat-square">
  <img alt="Status: pre-alpha" src="https://img.shields.io/badge/status-pre--alpha-d69e2e?style=flat-square">
  <img alt="Synthetic data only" src="https://img.shields.io/badge/data-synthetic%20only-e04f4f?style=flat-square">
</p>

> [!WARNING]
> **Pre-alpha. Synthetic data only.** HealthCraft is not certified health IT (ONC/ASTP), not
> cleared by the FDA or any other regulator, and must not be used for patient care or to store
> real patient data. See [`NOTICE`](NOTICE) and [`SECURITY.md`](SECURITY.md).

## Why

Health systems document care in one place and do research somewhere else. Getting data from the
chart to a study usually means a request queue, a custom extract, hand de-identification and an
export that is stale by the time it arrives. HealthCraft builds research into the record itself:

- **The record is an append-only event stream.** Nothing is edited or deleted; corrections are new
  events with a reason. That is how medical records are meant to work, and it gives a complete
  history and audit trail for free.
- **Everything else is a projection.** The clinician's chart, the FHIR R4 API, OMOP CDM research
  tables and Safe Harbor de-identified data sets are all computed from the same events, with no
  nightly ETL.
- **Everything is a command.** Every action (`patient.register`, `observation.record`,
  `research.deidentify`) is one authorized, audited command, whether it comes from the desktop UI,
  the HTTP API, the CLI or, later, an AI agent over MCP.

## What works today

| Area | Status |
|---|---|
| Event-sourced clinical record (patients, encounters, observations, problem list, signed notes, amendments, entered-in-error) | ✅ with invariants enforced on every write |
| Optimistic concurrency (two clinicians can never silently overwrite each other) | ✅ |
| Role-based access (clinician, researcher, admin, system) on every command | ✅ |
| Tamper-evident, hash-chained access log of every call, including refusals | ✅ |
| FHIR R4 Patient, Encounter, Observation, Condition; `Patient/$everything` | ✅ read |
| FHIR bundle import (Synthea synthetic patients) | ✅ |
| HL7 v2 parser (hardened) and ORU^R01 lab-result import | ✅ |
| OMOP CDM v5.4 export (person, visit, measurement, observation, condition) as CSV | ✅ source codes; standard concepts need the OHDSI vocabulary |
| HIPAA Safe Harbor de-identification (dates to year, 90+ ages, ZIP3, free text removed, random research ids) | ✅ |
| Clinician UI (egui), desktop and browser (WebAssembly) | ✅ patient banner; Summary, Encounters, Notes and Results (flowsheet) tabs; start and end visits; write and sign notes; de-identified research view; access log; demo role switcher |
| HTTP API (FHIR routes + command dispatch, bearer tokens) | ✅ development-grade auth |
| Durable storage, SMART on FHIR / OAuth, orders, medications, scheduling, billing, e-prescribing | ⏳ see [ROADMAP.md](ROADMAP.md) |

## Get started

You need a Rust toolchain (1.95+).

```sh
cargo test --workspace                 # unit and integration tests, including a fuzz pass over every command

# Desktop app, preloaded with synthetic patients
cargo run -p healthcraft

# Browser app (needs wasm-bindgen-cli at the version in Cargo.lock)
rustup target add wasm32-unknown-unknown
cargo xtask web && python3 -m http.server -d dist/web

# HTTP API
export HEALTHCRAFT_TOKEN_CLINICIAN=$(openssl rand -hex 32)
cargo run -p healthcraft-server -- --import fixtures/synthetic-bundle.json
curl -H "Authorization: Bearer $HEALTHCRAFT_TOKEN_CLINICIAN" \
  http://127.0.0.1:8080/fhir/r4/Patient/8f1d7c2e-0000-4000-8000-000000000001

# Command line
cargo run -p healthcraft-cli -- import fixtures/synthetic-bundle.json
cargo run -p healthcraft-cli -- omop fixtures/synthetic-bundle.json --out target/omop
cargo run -p healthcraft-cli -- deid fixtures/synthetic-bundle.json
cargo run -p healthcraft-cli -- hl7 fixtures/synthetic-oru.hl7
cargo run -p healthcraft-cli -- commands
```

For realistic volumes, generate synthetic patients with
[Synthea](https://github.com/synthetichealth/synthea) and pass its `output/fhir/*.json` files to
`--import` or the CLI.

On Linux the desktop app needs `libxkbcommon-x11` and a GPU (or Mesa) driver at runtime.

## Under the hood

```text
crates/
  core terminology hl7v2   L0  ids, actors, clocks, limits · code systems · HL7 v2 parser
  record audit             L1  the event-sourced record and chart · hash-chained access log
  store                    L2  append-only storage with optimistic concurrency
  fhir research            L3  FHIR R4 projection + bundle import · OMOP CDM + Safe Harbor
  engine                   L4  Session + command registry (authorize, run, audit)
  api ui-egui              L5  HTTP API (axum) · clinician UI (egui)
apps/
  healthcraft              desktop app (eframe on wgpu)
  healthcraft-server       HTTP API server
  healthcraft-cli          command line
  healthcraft-web          the same UI in the browser (WebAssembly, WebGL2)
xtask/                     cargo xtask layers | wasm | web | ci
```

- **Rust only.** No hand-written JavaScript, no webview shells. The UI is egui, natively on the
  desktop and compiled to WebAssembly in the browser; `wasm-bindgen` generates the small loader.
- **Never crash.** Non-test code may not `unwrap`, `expect`, `panic!` or index unchecked (enforced by
  clippy), and `unsafe` is forbidden workspace-wide.
- **Layering is enforced** by `cargo xtask layers`: the domain (L0–L4) cannot depend on UI, async or
  HTTP crates.

Details: [docs/architecture.md](docs/architecture.md), [docs/decisions.md](docs/decisions.md).

## Licensing

HealthCraft's code is dual-licensed under **[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)**,
at your option, the Rust ecosystem's standard. Apache-2.0 adds an explicit patent grant.

Clinical terminologies are separate: CPT (AMA) and SNOMED CT are licensed by their owners, and no
terminology content is bundled. Each deployment loads the files it is licensed to use. See
[`NOTICE`](NOTICE).

## The Crafting Apps

HealthCraft follows the engineering conventions of the Crafting Apps (PhotoCraft and its
siblings): pure Rust, egui, a layered crate workspace, every action a command, and a strict
never-crash policy. It is an independent project and is not (yet) part of the official ArtCraft
suite.

## Contributing

Read [AGENTS.md](AGENTS.md) (for people and AI agents alike) and [CONTRIBUTING.md](CONTRIBUTING.md).
Never put real patient data in an issue, a test or a commit.
