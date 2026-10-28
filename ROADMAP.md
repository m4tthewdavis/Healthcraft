# HealthCraft roadmap

**Stage: pre-alpha (M0 done).** The record, its invariants, the interfaces in and out, and the
research projections exist and are tested on synthetic data. Nothing is durable yet, and nothing
here is for patient care.

## Strategy

HealthCraft aims to become a full EHR, but it starts where incumbents are weakest and research
matters most, then grows outward:

1. **Clinical research sites:** the clinics and networks that run trials. They juggle an EHR, a
   trial management system, electronic data capture and spreadsheets. One research-native record
   replaces several tools, and they don't need hospital-scale procurement.
2. **Specialty outpatient practices** (oncology, cardiology, neurology), where care and research
   overlap.
3. **Community and rural hospitals**, which today get a large system's EHR on that system's terms.
4. **Academic medical centers**, by then as the EHR whose research data is already clean.

**Open source, sustainable.** The code is free (MIT OR Apache-2.0). Revenue comes from what
clinics and hospitals actually buy: managed HIPAA-compliant hosting with a Business Associate
Agreement, a supported and **certified** edition (certification applies to a vendor's specific
release, not to forks), implementation and data migration, support contracts, and grants for open
health infrastructure.

## Milestones

| | Milestone | Status |
|---|---|---|
| M0 | **Foundations:** event-sourced record, store with optimistic concurrency, command engine with roles and a hash-chained access log, FHIR R4 read + bundle import, HL7 v2 ORU import, OMOP export, Safe Harbor de-identification, egui chart, HTTP API, CLI, CI | ✅ |
| M1 | **Durable and secure:** PostgreSQL event store; durable, write-once access log; encryption at rest; TLS; SMART on FHIR / OAuth 2.0 sign-in; per-patient consent and care-team scoping; break-the-glass | next |
| M2 | **Usable outpatient chart:** write workflows in the UI (register, visits, vitals, problem list); clinical notes; medications and allergies; orders and results; scheduling; LOINC/SNOMED/RxNorm loaders; HL7 v2 ADT and ORM | started: visits and signed notes in the UI |
| M3 | **Research module:** cohort builder over the live OMOP projection; OHDSI vocabulary mapping; trial eligibility matching; consent tracking; electronic data capture for studies; de-identified exports with provenance | |
| M4 | **Pilot** with one clinical research site on synthetic, then real, data (after M1 security review) | |
| M5 | **Revenue cycle:** charge capture, X12 837/835 claims, eligibility (270/271) | |
| M6 | **Certification:** ONC Health IT certification (US Core, USCDI, SMART App Launch, Bulk FHIR `$export`, quality measures); e-prescribing (Surescripts) and EPCS; SOC 2 | |
| M7 | **Agents:** MCP server over the same commands; agents draft, clinicians sign | |
| M8 | **Browser build** of the clinician UI (WebAssembly) | ✅ demo (in-memory, synthetic); a hosted build follows M1 |

## Known gaps in what exists

- FHIR: read-only; no search parameters beyond listing patients; no CapabilityStatement; component
  observations (blood pressure panels) are skipped on import; notes are not exported as
  DocumentReference yet.
- Notes: no addenda or co-signature yet; signing is final.
- Store: replays the whole stream on every append (fine for now; snapshots come with PostgreSQL).
- De-identification: Safe Harbor only; no expert-determination support; the ZIP3 list is from the
  2000 Census as published by HHS and needs re-checking.
- UI: read-only chart; no accessibility audit yet (egui exposes AccessKit; screen-reader testing is
  part of M2).
