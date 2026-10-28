# healthcraft-terminology

Clinical code systems and the codes in them. **No terminology content ships in this crate or this
repository**: several code systems are licensed (see `NOTICE` at the repository root), so each
deployment loads the files it is licensed to use.

| System | URI | License | Bundled |
|---|---|---|---|
| LOINC | `http://loinc.org` | Free, LOINC license | No (load `Loinc.csv`) |
| SNOMED CT | `http://snomed.info/sct` | SNOMED International / UMLS license | No |
| RxNorm | `http://www.nlm.nih.gov/research/umls/rxnorm` | Free (some sources UMLS-restricted) | No |
| ICD-10-CM | `http://hl7.org/fhir/sid/icd-10-cm` | Free (CDC/NCHS) | No |
| CPT | `http://www.ama-assn.org/go/cpt` | **AMA license required** | Never |

Today the crate loads a simple tab-separated file (`code<TAB>display` per line) through
`InMemoryTerminology::load_tsv`. Native loaders for LOINC's CSV and SNOMED's RF2 release files are on
the roadmap.

This is a standalone crate: it depends on no other HealthCraft crate and can be published on its own.
