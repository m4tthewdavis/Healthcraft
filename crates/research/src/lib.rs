//! Research is a projection of the record, not an extract of it.
//!
//! - [`omop`]: the chart as OMOP Common Data Model v5.4 rows, the format OHDSI's research tools
//!   (ATLAS, HADES) and most multi-site studies use.
//! - [`deid`]: HIPAA Safe Harbor de-identification (45 CFR 164.514(b)(2)).
//! - [`csv`]: writing rows as RFC 4180 CSV.
//!
//! Because both are computed from the same event stream the chart is, a study sees data that is
//! current as of the last event, with no nightly ETL in between.

pub mod csv;
pub mod deid;
pub mod omop;
