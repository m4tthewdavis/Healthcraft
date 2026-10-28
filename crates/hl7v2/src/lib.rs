//! HL7 version 2 messages: the pipe-delimited format hospitals still use for most lab results,
//! admissions (ADT) and orders.
//!
//! Standalone: depends on no other HealthCraft crate. Every input is treated as hostile: sizes are
//! capped, delimiters are validated, and malformed messages return [`Hl7Error`], never a panic.
//!
//! ```
//! let msg = healthcraft_hl7v2::Message::parse(
//!     "MSH|^~\\&|LAB|HOSP|HC|CLINIC|20261027120000||ORU^R01|1|P|2.5.1\rPID|1||42^^^HOSP^MR||DOE^JANE",
//! )?;
//! assert_eq!(msg.message_type(), Some(("ORU", "R01")));
//! assert_eq!(msg.segment("PID").and_then(|pid| pid.component(5, 2)), Some("JANE"));
//! # Ok::<(), healthcraft_hl7v2::Hl7Error>(())
//! ```

use serde::Serialize;

pub mod oru;

/// Largest message accepted, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;
/// Most segments in one message.
pub const MAX_SEGMENTS: usize = 10_000;
/// Most fields in one segment, repetitions in one field, components in one repetition, and
/// subcomponents in one component.
pub const MAX_PARTS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Hl7Error {
    #[error("message is larger than {MAX_MESSAGE_BYTES} bytes")]
    TooLarge,
    #[error("message is empty")]
    Empty,
    #[error("message must start with an MSH segment")]
    MissingMsh,
    #[error("invalid delimiters in MSH: {0}")]
    BadDelimiters(String),
    #[error("segment {index}: invalid segment id `{id}`")]
    BadSegmentId { index: usize, id: String },
    #[error("too many {0}")]
    TooMany(&'static str),
}

/// The five delimiter characters a message declares in its MSH segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Delimiters {
    pub field: char,
    pub component: char,
    pub repetition: char,
    pub escape: Option<char>,
    pub subcomponent: Option<char>,
}

impl Default for Delimiters {
    fn default() -> Self {
        Self { field: '|', component: '^', repetition: '~', escape: Some('\\'), subcomponent: Some('&') }
    }
}

/// One field: repetitions of components of subcomponents, already unescaped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Field {
    pub repetitions: Vec<Vec<Vec<String>>>,
}

impl Field {
    fn raw(value: &str) -> Self {
        Self { repetitions: vec![vec![vec![value.to_owned()]]] }
    }

    /// The first subcomponent of component `n` (1-based) of the first repetition.
    pub fn component(&self, n: usize) -> Option<&str> {
        let index = n.checked_sub(1)?;
        self.repetitions.first()?.get(index)?.first().map(String::as_str)
    }

    /// The first value in the field (component 1 of repetition 1).
    pub fn value(&self) -> &str {
        self.component(1).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.repetitions.iter().flatten().flatten().all(String::is_empty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub id: String,
    /// `fields[0]` is field 1 (`PID-1`). For MSH, field 1 is the field separator itself.
    pub fields: Vec<Field>,
}

impl Segment {
    /// Field `n` (1-based, as HL7 numbers them: `PID-5` is `field(5)`).
    pub fn field(&self, n: usize) -> Option<&Field> {
        self.fields.get(n.checked_sub(1)?)
    }

    /// Component `c` of field `n`, both 1-based (`PID-5.2` is `component(5, 2)`). Empty values are
    /// `None`.
    pub fn component(&self, n: usize, c: usize) -> Option<&str> {
        self.field(n)?.component(c).filter(|v| !v.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    pub delimiters: Delimiters,
    pub segments: Vec<Segment>,
}

impl Message {
    pub fn parse(text: &str) -> Result<Self, Hl7Error> {
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(Hl7Error::TooLarge);
        }
        let mut lines = text.split(['\r', '\n']).filter(|l| !l.trim().is_empty());
        let Some(header) = lines.next() else {
            return Err(Hl7Error::Empty);
        };
        let header = header.trim_start_matches('\u{feff}');
        let Some(after_msh) = header.strip_prefix("MSH") else {
            return Err(Hl7Error::MissingMsh);
        };
        let delimiters = parse_delimiters(after_msh)?;

        let mut segments = vec![parse_msh(after_msh, &delimiters)?];
        for (index, line) in lines.enumerate() {
            if segments.len() >= MAX_SEGMENTS {
                return Err(Hl7Error::TooMany("segments"));
            }
            segments.push(parse_segment(index.saturating_add(2), line, &delimiters)?);
        }
        Ok(Self { delimiters, segments })
    }

    /// The first segment with this id.
    pub fn segment(&self, id: &str) -> Option<&Segment> {
        self.segments.iter().find(|s| s.id == id)
    }

    /// Every segment with this id, in order.
    pub fn segments_named<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Segment> + 'a {
        self.segments.iter().filter(move |s| s.id == id)
    }

    /// MSH-9: message code and trigger event, e.g. `("ORU", "R01")`.
    pub fn message_type(&self) -> Option<(&str, &str)> {
        let msh = self.segment("MSH")?;
        Some((msh.component(9, 1)?, msh.component(9, 2).unwrap_or("")))
    }

    /// MSH-10: the sender's message control id.
    pub fn control_id(&self) -> Option<&str> {
        self.segment("MSH")?.component(10, 1)
    }
}

fn parse_delimiters(after_msh: &str) -> Result<Delimiters, Hl7Error> {
    let mut chars = after_msh.chars();
    let field = chars.next().ok_or_else(|| Hl7Error::BadDelimiters("no field separator".into()))?;
    let encoding: Vec<char> = chars.take_while(|c| *c != field).collect();
    let mut enc = encoding.iter().copied();
    let component = enc.next().ok_or_else(|| Hl7Error::BadDelimiters("no component separator".into()))?;
    let repetition = enc.next().ok_or_else(|| Hl7Error::BadDelimiters("no repetition separator".into()))?;
    let escape = enc.next();
    let subcomponent = enc.next();
    // HL7 2.7 adds a fifth (truncation) character; anything beyond that is malformed.
    if encoding.len() > 5 {
        return Err(Hl7Error::BadDelimiters("too many encoding characters".into()));
    }
    let all: Vec<char> = [Some(field), Some(component), Some(repetition), escape, subcomponent].into_iter().flatten().collect();
    for (i, c) in all.iter().enumerate() {
        if c.is_alphanumeric() || c.is_whitespace() || c.is_control() {
            return Err(Hl7Error::BadDelimiters(format!("`{}` cannot be a delimiter", c.escape_default())));
        }
        if all.iter().skip(i.saturating_add(1)).any(|other| other == c) {
            return Err(Hl7Error::BadDelimiters(format!("`{c}` is used twice")));
        }
    }
    Ok(Delimiters { field, component, repetition, escape, subcomponent })
}

fn parse_msh(after_msh: &str, d: &Delimiters) -> Result<Segment, Hl7Error> {
    // "|^~\&|SENDER|..." splits into ["", "^~\&", "SENDER", ...]: MSH-1 is the separator itself and
    // MSH-2 the encoding characters, both kept verbatim.
    let mut parts = after_msh.split(d.field).skip(1);
    let encoding = parts.next().unwrap_or("");
    let mut fields = vec![Field::raw(&d.field.to_string()), Field::raw(encoding)];
    for part in parts {
        if fields.len() >= MAX_PARTS {
            return Err(Hl7Error::TooMany("fields"));
        }
        fields.push(parse_field(part, d)?);
    }
    Ok(Segment { id: "MSH".into(), fields })
}

fn parse_segment(index: usize, line: &str, d: &Delimiters) -> Result<Segment, Hl7Error> {
    let mut parts = line.split(d.field);
    let id = parts.next().unwrap_or("").trim().to_owned();
    if id.chars().count() != 3 || !id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(Hl7Error::BadSegmentId { index, id: id.chars().take(16).collect() });
    }
    let mut fields = Vec::new();
    for part in parts {
        if fields.len() >= MAX_PARTS {
            return Err(Hl7Error::TooMany("fields"));
        }
        fields.push(parse_field(part, d)?);
    }
    Ok(Segment { id, fields })
}

fn parse_field(text: &str, d: &Delimiters) -> Result<Field, Hl7Error> {
    let mut repetitions = Vec::new();
    for rep in text.split(d.repetition) {
        if repetitions.len() >= MAX_PARTS {
            return Err(Hl7Error::TooMany("repetitions"));
        }
        let mut components = Vec::new();
        for comp in rep.split(d.component) {
            if components.len() >= MAX_PARTS {
                return Err(Hl7Error::TooMany("components"));
            }
            let subs: Vec<String> = match d.subcomponent {
                Some(sub) => comp.split(sub).map(|s| unescape(s, d)).collect(),
                None => vec![unescape(comp, d)],
            };
            if subs.len() > MAX_PARTS {
                return Err(Hl7Error::TooMany("subcomponents"));
            }
            components.push(subs);
        }
        repetitions.push(components);
    }
    Ok(Field { repetitions })
}

/// Decodes HL7 escape sequences (`\F\`, `\S\`, `\T\`, `\R\`, `\E\`, `\.br\`). Sequences this parser
/// does not interpret (formatting, hex, character sets) are kept verbatim.
pub fn unescape(text: &str, d: &Delimiters) -> String {
    let Some(esc) = d.escape else {
        return text.to_owned();
    };
    if !text.contains(esc) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(esc) {
        out.push_str(rest.get(..start).unwrap_or(""));
        let after = rest.get(start.saturating_add(esc.len_utf8())..).unwrap_or("");
        let Some(end) = after.find(esc) else {
            // Unterminated: keep the rest as it is.
            out.push(esc);
            out.push_str(after);
            return out;
        };
        let name = after.get(..end).unwrap_or("");
        let decoded = match name {
            "F" => Some(d.field.to_string()),
            "S" => Some(d.component.to_string()),
            "R" => Some(d.repetition.to_string()),
            "E" => Some(esc.to_string()),
            "T" => d.subcomponent.map(|c| c.to_string()),
            ".br" => Some("\n".to_owned()),
            _ => None,
        };
        match decoded {
            Some(s) => out.push_str(&s),
            None => {
                out.push(esc);
                out.push_str(name);
                out.push(esc);
            }
        }
        rest = after.get(end.saturating_add(esc.len_utf8())..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const ORU: &str = "MSH|^~\\&|LAB|HOSPITAL|HEALTHCRAFT|CLINIC|20261027120000||ORU^R01|MSG00001|P|2.5.1\r\
PID|1||12345^^^HOSP^MR||TESTPATIENT^SYNTHETIC^Q||19800101|F\r\
OBR|1||ORD1|4548-4^Hemoglobin A1c^LN\r\
OBX|1|NM|4548-4^Hemoglobin A1c^LN||7.2|%^percent^UCUM|4.0-5.6|H|||F\r\
OBX|2|ST|8251-1^Service comment^LN||Fasting \\T\\ hydrated\\.br\\see \\F\\ note||||||F\r";

    #[test]
    fn parses_an_oru_message() {
        let msg = Message::parse(ORU).unwrap();
        assert_eq!(msg.delimiters, Delimiters::default());
        assert_eq!(msg.segments.len(), 5);
        assert_eq!(msg.message_type(), Some(("ORU", "R01")));
        assert_eq!(msg.control_id(), Some("MSG00001"));
        let msh = msg.segment("MSH").unwrap();
        assert_eq!(msh.field(1).unwrap().value(), "|");
        assert_eq!(msh.field(2).unwrap().value(), "^~\\&");
        assert_eq!(msh.component(12, 1), Some("2.5.1"));
        let pid = msg.segment("PID").unwrap();
        assert_eq!(pid.component(3, 1), Some("12345"));
        assert_eq!(pid.component(3, 5), Some("MR"));
        assert_eq!(pid.component(5, 1), Some("TESTPATIENT"));
        assert_eq!(pid.component(5, 2), Some("SYNTHETIC"));
        assert_eq!(pid.component(3, 2), None);
        assert_eq!(msg.segments_named("OBX").count(), 2);
    }

    #[test]
    fn unescapes_values() {
        let msg = Message::parse(ORU).unwrap();
        let note = msg.segments_named("OBX").nth(1).unwrap();
        assert_eq!(note.component(5, 1), Some("Fasting & hydrated\nsee | note"));
        let d = Delimiters::default();
        assert_eq!(unescape("a\\S\\b\\R\\c\\E\\", &d), "a^b~c\\");
        assert_eq!(unescape("hex \\X0D\\ kept", &d), "hex \\X0D\\ kept");
        assert_eq!(unescape("open \\F", &d), "open \\F");
    }

    #[test]
    fn accepts_lf_and_crlf_line_endings_and_custom_delimiters() {
        let text = "MSH#*@!$#A#B\r\nPID#1##7*X\nZZ1#custom";
        let msg = Message::parse(text).unwrap();
        assert_eq!(msg.delimiters.field, '#');
        assert_eq!(msg.delimiters.component, '*');
        assert_eq!(msg.segment("PID").unwrap().component(3, 2), Some("X"));
        assert_eq!(msg.segment("ZZ1").unwrap().component(1, 1), Some("custom"));
    }

    #[test]
    fn rejects_malformed_messages() {
        assert_eq!(Message::parse(""), Err(Hl7Error::Empty));
        assert_eq!(Message::parse("\r\n\r"), Err(Hl7Error::Empty));
        assert_eq!(Message::parse("PID|1"), Err(Hl7Error::MissingMsh));
        assert!(matches!(Message::parse("MSH"), Err(Hl7Error::BadDelimiters(_))));
        assert!(matches!(Message::parse("MSH|"), Err(Hl7Error::BadDelimiters(_))));
        assert!(matches!(Message::parse("MSH|^^"), Err(Hl7Error::BadDelimiters(_))));
        assert!(matches!(Message::parse("MSHa^~\\&"), Err(Hl7Error::BadDelimiters(_))));
        assert!(matches!(Message::parse("MSH|^~\\&|\rpid|1"), Err(Hl7Error::BadSegmentId { index: 2, .. })));
        assert_eq!(Message::parse(&"x".repeat(MAX_MESSAGE_BYTES + 1)), Err(Hl7Error::TooLarge));
    }

    #[test]
    fn caps_parts_per_level() {
        let many_fields = format!("MSH|^~\\&\rPID{}", "|".repeat(MAX_PARTS + 1));
        assert_eq!(Message::parse(&many_fields), Err(Hl7Error::TooMany("fields")));
        let many_reps = format!("MSH|^~\\&\rPID|{}", "~".repeat(MAX_PARTS + 1));
        assert_eq!(Message::parse(&many_reps), Err(Hl7Error::TooMany("repetitions")));
        let many_segments = format!("MSH|^~\\&{}", "\rPID|1".repeat(MAX_SEGMENTS + 1));
        assert_eq!(Message::parse(&many_segments), Err(Hl7Error::TooMany("segments")));
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        // A cheap deterministic fuzz pass; a cargo-fuzz target is on the roadmap.
        let alphabet: Vec<char> = "MSH|^~\\&\r\nPIDOBX01 éZ".chars().collect();
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..5_000 {
            let len = (state % 64) as usize;
            let mut s = String::from("MSH");
            for _ in 0..len {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                s.push(alphabet[(state % alphabet.len() as u64) as usize]);
            }
            let _ = Message::parse(&s);
        }
    }
}
