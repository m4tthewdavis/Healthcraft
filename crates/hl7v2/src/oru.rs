//! Lab results from ORU^R01 messages: each OBX segment becomes one [`LabResult`].

use serde::Serialize;

use crate::{Hl7Error, Message};

/// One observation from an OBX segment, with codes as the sender wrote them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabResult {
    /// OBX-3.1, e.g. a LOINC code.
    pub code: String,
    /// OBX-3.2.
    pub display: Option<String>,
    /// OBX-3.3, the coding system name (`LN` is LOINC).
    pub coding_system: Option<String>,
    pub value: LabValue,
    /// OBX-6.1.
    pub units: Option<String>,
    /// OBX-7.
    pub reference_range: Option<String>,
    /// OBX-8 (`H`, `L`, `A`, …).
    pub abnormal_flag: Option<String>,
    /// OBX-11 (`F` final, `P` preliminary, `C` corrected, …).
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum LabValue {
    Numeric(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OruError {
    #[error(transparent)]
    Parse(#[from] Hl7Error),
    #[error("not an ORU message (MSH-9 is {0})")]
    NotOru(String),
    #[error("OBX {index}: {reason}")]
    BadObx { index: usize, reason: String },
}

/// The lab results in an ORU message, in order. The patient (PID) is matched by the caller.
pub fn lab_results(msg: &Message) -> Result<Vec<LabResult>, OruError> {
    match msg.message_type() {
        Some(("ORU", _)) => {}
        other => return Err(OruError::NotOru(other.map(|(code, event)| format!("{code}^{event}")).unwrap_or_else(|| "missing".into()))),
    }
    let mut results = Vec::new();
    for (index, obx) in msg.segments_named("OBX").enumerate() {
        let index = index.saturating_add(1);
        let Some(code) = obx.component(3, 1) else {
            return Err(OruError::BadObx { index, reason: "OBX-3 (observation identifier) is empty".into() });
        };
        let raw = obx.field(5).map(|f| f.value()).unwrap_or("");
        let value = match obx.component(2, 1) {
            Some("NM" | "SN") => match raw.trim().parse::<f64>() {
                Ok(v) if v.is_finite() => LabValue::Numeric(v),
                _ => return Err(OruError::BadObx { index, reason: format!("OBX-5 `{}` is not a number", raw.chars().take(32).collect::<String>()) }),
            },
            _ => LabValue::Text(raw.to_owned()),
        };
        let text = |n: usize, c: usize| obx.component(n, c).map(str::to_owned);
        results.push(LabResult {
            code: code.to_owned(),
            display: text(3, 2),
            coding_system: text(3, 3),
            value,
            units: text(6, 1),
            reference_range: text(7, 1),
            abnormal_flag: text(8, 1),
            status: text(11, 1),
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::ORU;

    #[test]
    fn extracts_lab_results() {
        let results = lab_results(&Message::parse(ORU).unwrap()).unwrap();
        assert_eq!(results.len(), 2);
        let a1c = &results[0];
        assert_eq!(a1c.code, "4548-4");
        assert_eq!(a1c.coding_system.as_deref(), Some("LN"));
        assert_eq!(a1c.value, LabValue::Numeric(7.2));
        assert_eq!(a1c.units.as_deref(), Some("%"));
        assert_eq!(a1c.abnormal_flag.as_deref(), Some("H"));
        assert_eq!(a1c.status.as_deref(), Some("F"));
        assert!(matches!(results[1].value, LabValue::Text(_)));
    }

    #[test]
    fn rejects_non_oru_and_bad_obx() {
        let adt = Message::parse("MSH|^~\\&|||||||ADT^A01|1|P|2.5").unwrap();
        assert_eq!(lab_results(&adt), Err(OruError::NotOru("ADT^A01".into())));
        let bad = Message::parse("MSH|^~\\&|||||||ORU^R01|1|P|2.5\rOBX|1|NM|1-8||high").unwrap();
        assert!(matches!(lab_results(&bad), Err(OruError::BadObx { index: 1, .. })));
        let nan = Message::parse("MSH|^~\\&|||||||ORU^R01|1|P|2.5\rOBX|1|NM|1-8||NaN").unwrap();
        assert!(matches!(lab_results(&nan), Err(OruError::BadObx { .. })));
        let no_code = Message::parse("MSH|^~\\&|||||||ORU^R01|1|P|2.5\rOBX|1|NM|||1").unwrap();
        assert!(matches!(lab_results(&no_code), Err(OruError::BadObx { .. })));
    }
}
