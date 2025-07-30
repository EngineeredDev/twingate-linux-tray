#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet_with_name() {
        let result = greet("Alice");
        assert_eq!(result, "Hello, Alice! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_with_empty_string() {
        let result = greet("");
        assert_eq!(result, "Hello, ! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_with_special_characters() {
        let result = greet("José & María");
        assert_eq!(result, "Hello, José & María! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_with_unicode() {
        let result = greet("用户");
        assert_eq!(result, "Hello, 用户! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_with_numbers() {
        let result = greet("User123");
        assert_eq!(result, "Hello, User123! You've been greeted from Rust!");
    }

    #[test]
    fn test_greet_return_type() {
        let result = greet("test");
        assert!(result.is_ascii());
        assert!(result.contains("Hello"));
        assert!(result.contains("test"));
        assert!(result.contains("Rust"));
    }
}
