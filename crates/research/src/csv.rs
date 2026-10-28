//! RFC 4180 CSV, with spreadsheet formula injection neutralized in text cells.

/// A row type that can be written as CSV.
pub trait CsvRow {
    const HEADER: &'static [&'static str];
    fn cells(&self) -> Vec<String>;
}

/// The rows as CSV text: a header line, then one line per row, CRLF line endings.
pub fn write<T: CsvRow>(rows: &[T]) -> String {
    let mut out = T::HEADER.join(",");
    out.push_str("\r\n");
    for row in rows {
        let cells: Vec<String> = row.cells().iter().map(|c| quote(c)).collect();
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    out
}

fn quote(cell: &str) -> String {
    if cell.contains([',', '"', '\r', '\n']) { format!("\"{}\"", cell.replace('"', "\"\"")) } else { cell.to_owned() }
}

/// A free-text cell. Text starting with `=`, `+`, `-`, `@` or a tab is prefixed with `'` so a
/// spreadsheet shows it instead of running it as a formula. Numbers go through [`number`] instead.
pub fn text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{value}") } else { value.to_owned() }
}

pub fn number<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row(&'static str, i64);
    impl CsvRow for Row {
        const HEADER: &'static [&'static str] = &["name", "n"];
        fn cells(&self) -> Vec<String> {
            vec![text(self.0), number(Some(self.1))]
        }
    }

    #[test]
    fn quotes_and_neutralizes() {
        let csv = write(&[Row("plain", 1), Row("a,\"b\"\nc", -2), Row("=HYPERLINK(1)", 3)]);
        assert_eq!(csv, "name,n\r\nplain,1\r\n\"a,\"\"b\"\"\nc\",-2\r\n'=HYPERLINK(1),3\r\n");
    }
}
