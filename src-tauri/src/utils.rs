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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url_from_line_https() {
        let line = "Visit https://example.com for more info";
        let url = extract_url_from_line(line).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_from_line_http() {
        let line = "Visit http://example.com for more info";
        let url = extract_url_from_line(line).unwrap();
        assert_eq!(url, "http://example.com");
    }

    #[test]
    fn test_extract_url_from_line_with_trailing_punctuation() {
        let test_cases = vec![
            ("Visit https://example.com.", "https://example.com"),
            ("Visit https://example.com,", "https://example.com"),
            ("Visit https://example.com)", "https://example.com"),
            ("Visit https://example.com]", "https://example.com"),
            ("Visit https://example.com}", "https://example.com"),
            ("Visit https://example.com\"", "https://example.com"),
        ];

        for (input, expected) in test_cases {
            let url = extract_url_from_line(input).unwrap();
            assert_eq!(url, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_extract_url_from_line_complex_url() {
        let line = "Visit https://twingate.com/auth?token=abc123&redirect=true";
        let url = extract_url_from_line(line).unwrap();
        assert_eq!(url, "https://twingate.com/auth?token=abc123&redirect=true");
    }

    #[test]
    fn test_extract_url_from_line_multiple_urls() {
        let line = "Visit https://first.com and https://second.com";
        let url = extract_url_from_line(line).unwrap();
        assert_eq!(url, "https://first.com"); // Should return the first one
    }

    #[test]
    fn test_extract_url_from_line_no_url() {
        let line = "This line has no URL";
        let url = extract_url_from_line(line);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_from_line_short_url() {
        let line = "Visit http://a.co";
        let url = extract_url_from_line(line).unwrap();
        assert_eq!(url, "http://a.co");
    }

    #[test]
    fn test_extract_url_from_line_too_short() {
        let line = "Visit http://a"; // Less than 10 characters
        let url = extract_url_from_line(line);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_from_line_just_protocol() {
        let line = "Visit https://";
        let url = extract_url_from_line(line);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_from_text_single_line() {
        let text = "Visit https://example.com for more info";
        let url = extract_url_from_text(text).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_from_text_multiple_lines() {
        let text = "Line 1 has no URL\nLine 2 has https://example.com\nLine 3 also has no URL";
        let url = extract_url_from_text(text).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_from_text_multiple_urls() {
        let text = "First line: https://first.com\nSecond line: https://second.com";
        let url = extract_url_from_text(text).unwrap();
        assert_eq!(url, "https://first.com"); // Should return the first one found
    }

    #[test]
    fn test_extract_url_from_text_no_url() {
        let text = "Line 1 has no URL\nLine 2 also has no URL\nLine 3 still has no URL";
        let url = extract_url_from_text(text);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_from_text_empty() {
        let text = "";
        let url = extract_url_from_text(text);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_with_pattern_basic() {
        let text = "Please visit: https://example.com to continue";
        let patterns = &["visit:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_with_pattern_multiple_patterns() {
        let text = "Please go to: https://example.com to continue";
        let patterns = &["visit:", "go to:", "open:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_with_pattern_case_insensitive() {
        let text = "Please VISIT: https://example.com to continue";
        let patterns = &["visit:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn test_extract_url_with_pattern_fallback_to_any_url() {
        let text = "Authentication required. URL: https://example.com";
        let patterns = &["visit:", "go to:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://example.com"); // Should find URL even without pattern match
    }

    #[test]
    fn test_extract_url_with_pattern_multiple_lines() {     
        let text = "Line 1: Authentication required\nLine 2: Please visit: https://auth.example.com\nLine 3: Complete the process";
        let patterns = &["visit:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://auth.example.com");
    }

    #[test]
    fn test_extract_url_with_pattern_no_match() {
        let text = "Please authenticate but no URL provided";
        let patterns = &["visit:", "go to:"];
        let url = extract_url_with_pattern(text, patterns);
        assert_eq!(url, None);
    }

    #[test]
    fn test_extract_url_with_pattern_empty_patterns() {
        let text = "Visit https://example.com";
        let patterns = &[];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://example.com"); // Should still find URL without patterns
    }

    #[test]
    fn test_extract_url_with_pattern_auth_scenario() {
        let text = "User authentication is required. Please navigate to: https://auth.twingate.com?token=abc123 to complete the authentication process.";
        let patterns = &["navigate to:", "visit:", "go to:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://auth.twingate.com?token=abc123");
    }

    #[test]
    fn test_extract_url_with_pattern_real_world_scenario() {
        let text = r#"
twingate status output:
Status: Authenticating
Please visit: https://mycompany.twingate.com/auth/device?code=ABCD1234&session=xyz789
to complete device authentication.
        "#;
        let patterns = &["visit:", "go to:", "open:"];
        let url = extract_url_with_pattern(text, patterns).unwrap();
        assert_eq!(url, "https://mycompany.twingate.com/auth/device?code=ABCD1234&session=xyz789");
    }

    #[test]
    fn test_url_extraction_edge_cases() {
        // Test URL at start of line
        let url = extract_url_from_line("https://example.com is the URL").unwrap();
        assert_eq!(url, "https://example.com");

        // Test URL at end of line
        let url = extract_url_from_line("The URL is https://example.com").unwrap();
        assert_eq!(url, "https://example.com");

        // Test URL with port
        let url = extract_url_from_line("Visit https://example.com:8080/path").unwrap();
        assert_eq!(url, "https://example.com:8080/path");

        // Test URL with fragment
        let url = extract_url_from_line("Visit https://example.com/page#section").unwrap();
        assert_eq!(url, "https://example.com/page#section");
    }
}