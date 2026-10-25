use crate::json::{field_bool, field_str};

// Only raw table requests align records; csv "," and "\t" select their field separator.
// Sample input: delimiter(r#"{"csv":",","raw":false}"#) returns None.
pub fn delimiter(request: &str) -> Option<u8> {
    if !field_bool(request, "raw") {
        return None;
    }
    match field_str(request, "csv").as_deref() {
        Some(",") => Some(b','),
        Some("\t") => Some(b'\t'),
        _ => None,
    }
}

pub struct Range {
    pub first: usize,
    pub count: usize,
    pub total: usize,
    pub records: usize,
    pub first_record: usize,
}

// Expand the physical-line window to whole RFC 4180 records and count their ordinals in the same scan of the held bytes.
// Sample input: align(b"id,note\n1,\"two\nlines\",9\n2,last\n", 3, 1, b',', None) has first 2, count 2, records 3 and first_record 2.
pub fn align(data: &[u8], first: usize, count: usize, delim: u8, record: Option<usize>) -> Range {
    let mut wanted_end = first.saturating_add(count.saturating_sub(1));
    let mut line = 1;
    let mut record_first = 1;
    let mut records = 0;
    let mut start = None;
    let mut end = None;
    let mut quoted = false;
    let mut field_start = true;
    // A UTF-8 BOM is a file prefix, not part of the first field.
    let mut at = if data.starts_with(b"\xef\xbb\xbf") { 3 } else { 0 };
    loop {
        let eof = at == data.len();
        let byte = data.get(at).copied().unwrap_or(b'\n');
        let last = line - usize::from(eof && (data.is_empty() || data.ends_with(b"\n")));
        if eof && record_first > last {
            break;
        }
        if eof || (byte == b'\n' && !quoted) {
            records += 1;
            let wanted = record.map_or(last >= first, |ordinal| records == ordinal);
            if start.is_none() && wanted {
                start = Some((record_first, records));
                if record.is_some() {
                    wanted_end = record_first.saturating_add(count.saturating_sub(1));
                }
            }
            if start.is_some() && end.is_none() && (last >= wanted_end || eof) {
                end = Some(last);
            }
            if eof {
                break;
            }
            record_first = line + 1;
            field_start = true;
        } else if quoted {
            if byte == b'"' {
                if data.get(at + 1) == Some(&b'"') {
                    at += 1;
                } else {
                    quoted = false;
                }
            }
        } else if byte == delim {
            field_start = true;
        } else if byte != b'\n' {
            if byte == b'"' && field_start {
                quoted = true;
            }
            field_start = false;
        }
        if byte == b'\n' {
            line += 1;
        }
        at += 1;
    }
    let total = line - usize::from(data.is_empty() || data.ends_with(b"\n"));
    let (first, first_record) = start.unwrap_or((if record.is_some() { total + 1 } else { first }, record.unwrap_or(records + 1)));
    let count = if start.is_some() { end.unwrap_or(total) - first + 1 } else { 0 };
    Range { first, count, total, records, first_record }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{plain, Ask};
    use crate::json::escape;
    use crate::jsondoc::{parse, Json};

    fn reply(data: &str, first: usize, count: usize, csv: &str) -> Json {
        let request = Ask::parse(&format!(
            r#"{{"text":"{}","raw":true,"csv":"{}","first":{first},"count":{count}}}"#,
            escape(data), escape(csv)
        ));
        parse(&plain(&request, data.as_bytes())).unwrap()
    }

    fn window(reply: &Json) -> (usize, usize) {
        (reply.get("first").and_then(Json::as_f64).unwrap() as usize,
         reply.get("lines").and_then(Json::as_array).unwrap().len())
    }

    #[test]
    fn code_csv_moves_first_back_inside_a_quoted_newline() {
        let data = "id,note\n1,\"two\nlines\",9\n2,last\n";
        assert_eq!(window(&reply(data, 3, 1, ",")), (2, 2));
        assert_eq!(window(&reply(data, 2, 1, ",")), (2, 2));
    }

    #[test]
    fn code_csv_continues_past_line_cap_to_a_record_end() {
        let data = format!("id,note\n1,\"open\n{}closed\",9\n2,last\n", "still\n".repeat(500));
        assert_eq!(window(&reply(&data, 1, 400, ",")), (1, 503));
    }

    #[test]
    fn code_csv_unterminated_quote_extends_to_file_end() {
        for data in ["id,note\n1,\"open\nstill", "id,note\n1,\"open\nstill\n"] {
            assert_eq!(window(&reply(data, 3, 1, ",")), (2, 2));
            assert_eq!(window(&reply(data, 1, 2, ",")), (1, 3));
        }
    }

    #[test]
    fn code_csv_doubled_quotes_keep_the_quoted_field_open() {
        let data = "id,note\n1,\"say \"\"hi\"\"\nnext\",9\n2,last\n";
        assert_eq!(window(&reply(data, 3, 1, ",")), (2, 2));
    }

    #[test]
    fn code_csv_bom_before_a_quoted_header_keeps_record_boundaries() {
        for separator in [",", "\t"] {
            let data = format!("\u{feff}\"Full\nname\"{separator}Amount\nAlice{separator}10\n");
            let result = reply(&data, 2, 1, separator);
            assert_eq!(window(&result), (1, 2), "the header is one multiline record");
            assert_eq!(result.get("records").and_then(Json::as_f64), Some(2.0));
            assert_eq!(result.get("firstRecord").and_then(Json::as_f64), Some(1.0));
            let body = reply(&data, 3, 1, separator);
            assert_eq!(body.get("firstRecord").and_then(Json::as_f64), Some(2.0));
        }
    }

    #[test]
    fn code_csv_a_quote_inside_an_unquoted_field_is_literal() {
        let data = "id,note\n1,5\" disk\n2,last\n";
        assert_eq!(window(&reply(data, 2, 1, ",")), (2, 1));
        assert_eq!(window(&reply(data, 3, 1, ",")), (3, 1));
    }

    #[test]
    fn code_csv_tsv_opens_quotes_after_tabs_and_keeps_commas_literal() {
        let data = "id\tnote\n1\t\"two\nlines\"\t9\n2,\"literal\tlast\n3\tdone\n";
        assert_eq!(window(&reply(data, 3, 1, "\t")), (2, 2));
        assert_eq!(window(&reply(data, 4, 1, "\t")), (4, 1));
    }

    #[test]
    fn code_csv_plain_requests_and_invalid_delimiters_keep_the_old_window() {
        let data = "id,note\n1,\"two\nlines\",9\n";
        for csv in ["", "|", "comma"] {
            assert_eq!(window(&reply(data, 3, 1, csv)), (3, 1));
        }
        assert_eq!(delimiter(r#"{"csv":",","raw":false}"#), None);
        let empty = align(b"", 1, 400, b',', None);
        let past = align(b"h\nv\n", 3, 400, b',', None);
        assert_eq!((empty.first, empty.count), (1, 0));
        assert_eq!((past.first, past.count), (3, 0));
    }

    #[test]
    fn code_csv_reports_record_count_and_first_ordinal() {
        let data = "id,note\n1,\"two\nlines\",9\n2,last\n";
        let result = reply(data, 3, 1, ",");
        assert_eq!(result.get("records").and_then(Json::as_f64), Some(3.0), "CSV records include the header, not continuation lines");
        assert_eq!(result.get("firstRecord").and_then(Json::as_f64), Some(2.0), "firstRecord is the containing record's ordinal");
        assert_eq!(result.get("total").and_then(Json::as_f64), Some(4.0), "total remains physical lines");
    }

    #[test]
    fn code_csv_accepts_a_record_ordinal_for_a_far_window() {
        let data = "id,note\n1,\"two\nlines\",9\n2,last\n";
        let request = Ask::parse(&format!(
            r#"{{"text":"{}","raw":true,"csv":",","record":3,"first":1,"count":1}}"#,
            escape(data)
        ));
        let result = parse(&plain(&request, data.as_bytes())).unwrap();
        assert_eq!(window(&result), (4, 1), "record 3 begins at physical line 4");
        assert_eq!(result.get("firstRecord").and_then(Json::as_f64), Some(3.0));
        assert_eq!(result.get("records").and_then(Json::as_f64), Some(3.0));
    }

    #[test]
    fn code_csv_counts_an_unterminated_record_and_tsv_newlines() {
        for (data, csv) in [
            ("id,note\n1,\"two\nlines\"\n2,\"last\nend\n", ","),
            ("id\tnote\n1\t\"two\nlines\"\n2\t\"last\nend", "\t"),
        ] {
            let result = reply(data, 5, 1, csv);
            assert_eq!(window(&result), (4, 2));
            assert_eq!(result.get("records").and_then(Json::as_f64), Some(3.0), "an open quote at EOF is one record");
            assert_eq!(result.get("firstRecord").and_then(Json::as_f64), Some(3.0));
            assert_eq!(result.get("total").and_then(Json::as_f64), Some(5.0));
        }
    }

    #[test]
    fn code_csv_empty_and_header_only_counts_have_no_phantom_record() {
        for (data, records) in [("", 0.0), ("h\n", 1.0), ("h", 1.0), ("h\n\n", 2.0)] {
            let result = reply(data, 1, 400, ",");
            assert_eq!(result.get("records").and_then(Json::as_f64), Some(records));
        }
        let request = Ask::parse(r#"{"text":"h\nv\n","raw":true}"#);
        let result = parse(&plain(&request, b"h\nv\n")).unwrap();
        assert!(result.get("records").is_none(), "non-CSV replies keep the line-only contract");
        assert!(result.get("firstRecord").is_none());
    }
}
