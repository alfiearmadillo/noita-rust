//! `data/translations/common.csv` reader: string keys (`$mat_water` without the `$`) to English text.

use std::collections::HashMap;

/// Splits CSV text into rows of fields; quoted fields may hold commas, newlines and `""`.
fn rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut field) = (Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Key to English text for every row whose English column is not empty. Column 1 is `en`.
pub fn parse_english(csv: &str) -> HashMap<String, String> {
    rows(csv.trim_start_matches('\u{feff}'))
        .into_iter()
        .filter_map(|mut row| {
            if row.len() < 2 || row[0].is_empty() || row[1].is_empty() {
                return None;
            }
            let text = row.swap_remove(1);
            Some((std::mem::take(&mut row[0]), text))
        })
        .collect()
}

/// The display name of a `ui_name` attribute such as `$mat_water`, if translated.
pub fn lookup<'a>(english: &'a HashMap<String, String>, ui_name: &str) -> Option<&'a str> {
    english
        .get(ui_name.trim().trim_start_matches('$'))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_commas_and_multiline_fields() {
        let csv = ",en,ru\r\nmat_water,Water,Вода\r\nnote,\"a, \"\"b\"\"\nc\",x\r\nempty,,y\r\n";
        let map = parse_english(csv);
        assert_eq!(map["mat_water"], "Water");
        assert_eq!(map["note"], "a, \"b\"\nc");
        assert!(!map.contains_key("empty"));
        assert_eq!(lookup(&map, "$mat_water"), Some("Water"));
        assert_eq!(lookup(&map, "$missing"), None);
    }
}
