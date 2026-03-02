pub fn extract_json_objects(raw: &str) -> Vec<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (i, b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if *b == b'\\' {
                escaped = true;
                continue;
            }
            if *b == b'"' {
                in_string = false;
            }
            continue;
        }

        if *b == b'"' {
            in_string = true;
            continue;
        }
        if *b == b'{' {
            if start.is_none() {
                start = Some(i);
            }
            depth += 1;
            continue;
        }
        if *b == b'}' {
            if depth == 0 {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    out.push(raw[s..=i].to_string());
                    start = None;
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::extract_json_objects;

    #[test]
    fn extracts_embedded_json_object() {
        let raw = r#"before {"a":1,"b":"{ok}"} after"#;
        let objects = extract_json_objects(raw);
        assert_eq!(objects, vec![r#"{"a":1,"b":"{ok}"}"#.to_string()]);
    }
}
