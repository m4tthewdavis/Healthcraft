# Decisions

Short records of choices that shape the codebase. Newest last.

### D1. Rust only, no webview (2026-10)

Health software must not crash or corrupt data, and Rust's type system and the workspace lints
(no `unwrap`, no unchecked indexing, no `unsafe`) make whole classes of failure unrepresentable.
Matching the Crafting Apps, the UI is native egui with no JavaScript, Tauri or Electron.
**Cost:** fewer ready-made UI components and a smaller hiring pool for front-end work; the
healthcare ecosystem in Rust is thin, so HealthCraft writes its own FHIR, HL7 v2 and terminology
layers.

### D2. The record is event-sourced (2026-10)

Medical records are legally append-only: corrections are amendments with a reason. Storing events
makes that the only possible behaviour, gives a full history and audit trail, and lets the chart,
FHIR, OMOP and de-identified views be projections, which is what makes HealthCraft
research-native.

### D3. MIT OR Apache-2.0 (2026-10)

Free and open source, permissive for the widest adoption by clinics, hospitals, researchers and
vendors. Dual licensing follows the Rust ecosystem; Apache-2.0 adds an explicit patent grant.
**Accepted trade-off:** anyone, including a competitor, may build a proprietary product on the
code. The business model relies on hosting, a certified edition, services and support rather than
on license restrictions.

### D4. No terminology content in the repository (2026-10)

CPT and SNOMED CT cannot be redistributed, and even free code systems have their own terms. The
repository ships code-system identifiers and loaders; each deployment loads what it is licensed to
use.

### D5. Time and ids are injected (2026-10)

The domain never reads the wall clock or a random source. Tests are exact, replays are
reproducible, and L0–L4 build for WebAssembly, where neither exists without JavaScript.

### D6. Coarse roles first, consent next (2026-10)

Four roles (clinician, researcher, admin, system) mapped to seven access kinds, decided in one
function. Per-patient consent, care-team scoping and break-the-glass come with M1, in the same
place.
