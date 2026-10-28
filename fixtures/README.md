# Fixtures

Test data. **Everything here is synthetic**: invented by hand in the shape of the bundles
[Synthea](https://github.com/synthetichealth/synthea) generates. No file in this repository may
contain real patient data, even "de-identified" (see `SECURITY.md`).

| File | What |
|---|---|
| `synthetic-bundle.json` | One synthetic patient: 2 encounters, 3 observations, 1 condition, plus a blood-pressure panel and an Organization the importer reports as skipped |
| `synthetic-oru.hl7` | An HL7 v2 ORU^R01 lab result for the same patient |

For larger data sets, generate patients with Synthea (`./run_synthea -p 100`) and import its
`output/fhir/*.json` with `healthcraft-cli import`.
