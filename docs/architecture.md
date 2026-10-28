# HealthCraft architecture

## The idea in one picture

```text
              commands (UI · HTTP API · CLI · later MCP)
                               │
                     Session::dispatch
              authorize (Role × Access) → run → audit
                               │
                    ┌──────────┴──────────┐
                    │  EventStore         │  append-only, one stream per patient,
                    │  (ClinicalEvent…)   │  optimistic concurrency, invariants checked
                    └──────────┬──────────┘
          ┌────────────────────┼─────────────────────┬──────────────────────┐
          ▼                    ▼                     ▼                      ▼
     Chart (clinician)   FHIR R4 resources     OMOP CDM rows        Safe Harbor data set
       record crate         fhir crate          research crate        research crate
```

Every view is a pure function of the event stream. Adding a view never needs a migration of the
record, and every view can be rebuilt from scratch.

## Layers

| Layer | Crates | May use |
|---|---|---|
| L0 | `core`; standalone `terminology`, `hl7v2` | std, serde, chrono, uuid; standalone crates use no workspace crate |
| L1 | `record`, `audit` | L0 |
| L2 | `store` | L0–L1 |
| L3 | `fhir`, `research` | L0–L2 |
| L4 | `engine` | L0–L3 |
| L5 | `api`, `ui-egui` | L0–L4, plus UI, async and HTTP crates |
| apps | `healthcraft`, `healthcraft-server`, `healthcraft-cli` | anything |

`cargo xtask layers` enforces this table (`xtask/src/layers.rs`). L0–L4 are synchronous, headless,
deterministic (time and ids are injected through `Clock` and `IdSource`) and compile to
`wasm32-unknown-unknown` (`cargo xtask wasm`).

## The record (`crates/record`)

- `ClinicalEvent`: `PatientRegistered`, `DemographicsAmended`, `EncounterOpened`,
  `EncounterClosed`, `ObservationRecorded`, `ObservationEnteredInError`, `ConditionRecorded`,
  `NoteSigned`.
- `RecordedEvent`: an event plus its 1-based sequence in the stream, time and `Actor`.
- `Chart::apply` validates an event against the current chart (registered first, encounters open
  before use, no duplicates, no results into a closed encounter, visits never end before they
  start, notes only on visits that exist) and is the only way state changes.
- Notes are free text, so de-identification never copies them (`research::deid` builds its output
  from an allow-list of fields).

## Storage (`crates/store`)

`EventStore::append(patient, expected_version, …)` fails with `Conflict` if the stream moved since
the writer read it, and applies new events to the chart before storing them, so the store never
holds history the record would reject. A batch is all-or-nothing.

## The engine (`crates/engine`)

`registry()` lists every `CommandSpec` (id, label, params doc, `Access`, `run`). `Session::dispatch`
looks the command up, checks `allowed(role, access)`, runs it, and appends one access-log entry
(success, denied or failed) with the actor, the command and the patient touched.

| Access | Clinician | Researcher | Admin | System |
|---|:-:|:-:|:-:|:-:|
| Meta (`commands.list`) | ✓ | ✓ | ✓ | ✓ |
| ReadChart | ✓ | | ✓ | ✓ |
| WriteChart | ✓ | | | ✓ |
| Import | | | ✓ | ✓ |
| Export (identified OMOP) | | | ✓ | ✓ |
| Research (de-identified) | | ✓ | ✓ | ✓ |
| Audit | | | ✓ | ✓ |

## Interfaces

- **In:** FHIR bundles (`fhir::import`, Synthea-shaped), HL7 v2 ORU^R01 (`hl7v2::oru`).
- **Out:** FHIR R4 (`fhir::export`), OMOP CDM v5.4 (`research::omop`), Safe Harbor
  (`research::deid`), CSV (`research::csv`, with spreadsheet formula injection neutralized).
