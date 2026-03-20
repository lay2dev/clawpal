pub fn normalize_fingerprint(input: &str) -> Option<String> {
    let compact: String = input
        .chars()
        .filter(|ch| *ch != ':' && !ch.is_ascii_whitespace())
        .collect();
    if compact.is_empty()
        || compact.len() % 2 != 0
        || !compact.chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return None;
    }
    let upper = compact.to_ascii_uppercase();
    let pairs = upper
        .as_bytes()
        .chunks(2)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<_>>();
    Some(pairs.join(":"))
}
