//! Lightweight JSON5 key extraction utilities.
//!
//! Extracted from doctor_assistant.rs for readability.

pub(crate) fn skip_json5_ws_and_comments(text: &str, mut index: usize) -> usize {
    let bytes = text.as_bytes();
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                index += 1;
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 < bytes.len() {
                    index += 2;
                }
            }
            _ => break,
        }
    }
    index
}

pub(crate) fn scan_json5_string_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let quote = *bytes.get(start)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

pub(crate) fn scan_json5_value_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let start = skip_json5_ws_and_comments(text, start);
    let first = *bytes.get(start)?;
    if first == b'"' || first == b'\'' {
        return scan_json5_string_end(text, start);
    }
    if first != b'{' && first != b'[' {
        let mut index = start;
        while index < bytes.len() {
            index = skip_json5_ws_and_comments(text, index);
            if index >= bytes.len() {
                break;
            }
            match bytes[index] {
                b',' | b'}' => break,
                b'"' | b'\'' => {
                    index = scan_json5_string_end(text, index)?;
                }
                _ => index += 1,
            }
        }
        return Some(index);
    }

    let mut stack = vec![first];
    let mut index = start + 1;
    while index < bytes.len() {
        index = skip_json5_ws_and_comments(text, index);
        if index >= bytes.len() {
            break;
        }
        match bytes[index] {
            b'"' | b'\'' => {
                index = scan_json5_string_end(text, index)?;
            }
            b'{' | b'[' => {
                stack.push(bytes[index]);
                index += 1;
            }
            b'}' => {
                let open = stack.pop()?;
                if open != b'{' {
                    return None;
                }
                index += 1;
                if stack.is_empty() {
                    return Some(index);
                }
            }
            b']' => {
                let open = stack.pop()?;
                if open != b'[' {
                    return None;
                }
                index += 1;
                if stack.is_empty() {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

pub(crate) fn extract_json5_top_level_value(text: &str, key: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        index = skip_json5_ws_and_comments(text, index);
        if index >= bytes.len() {
            break;
        }
        match bytes[index] {
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            b'"' | b'\'' if depth == 1 => {
                let end = scan_json5_string_end(text, index)?;
                let raw_key = &text[index + 1..end - 1];
                let after_key = skip_json5_ws_and_comments(text, end);
                if raw_key == key && bytes.get(after_key) == Some(&b':') {
                    let value_start = skip_json5_ws_and_comments(text, after_key + 1);
                    let value_end = scan_json5_value_end(text, value_start)?;
                    return Some(text[value_start..value_end].trim().to_string());
                }
                index = end;
            }
            b'"' | b'\'' => {
                index = scan_json5_string_end(text, index)?;
            }
            _ => index += 1,
        }
    }
    None
}

