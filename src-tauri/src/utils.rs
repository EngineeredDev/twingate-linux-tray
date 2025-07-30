pub fn extract_url_from_text(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(url) = extract_url_from_line(line) {
            return Some(url);
        }
    }
    None
}

pub fn extract_url_from_line(line: &str) -> Option<String> {
    if let Some(url_start) = line.find("http") {
        let url_part = &line[url_start..];
        let url_end = url_part.find(char::is_whitespace).unwrap_or(url_part.len());
        let url = url_part[..url_end].trim_end_matches(&['.', ',', ')', ']', '}', '"'][..]);
        
        if !url.is_empty() && (url.starts_with("https://") || url.starts_with("http://")) && url.len() > 10 {
            return Some(url.to_string());
        }
    }
    None
}

pub fn extract_url_with_pattern(text: &str, patterns: &[&str]) -> Option<String> {
    for line in text.lines() {
        for pattern in patterns {
            if let Some(pattern_pos) = line.to_lowercase().find(pattern) {
                let after_pattern = &line[pattern_pos + pattern.len()..];
                if let Some(url) = extract_url_from_line(after_pattern) {
                    return Some(url);
                }
            }
        }
        
        if let Some(url) = extract_url_from_line(line) {
            return Some(url);
        }
    }
    None
}