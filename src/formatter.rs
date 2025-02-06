pub fn split_safe_utf8(s: &str, max_bytes: usize) -> Result<Vec<&str>, &'static str> {
    if max_bytes == 0 {
        return Err("max_bytes must be greater than zero");
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < s.len() {
        let remaining = s.len() - start;

        // If the remaining part fits, add it and break.
        if remaining <= max_bytes {
            chunks.push(&s[start..]);
            break;
        }

        // Start with the candidate end index.
        let mut end = start + max_bytes;

        // If this index is not a valid boundary, move backward until it is.
        while !s.is_char_boundary(end) {
            end -= 1;
        }

        // If we scanned all the way back to start, the next character doesn't fit.
        if end == start {
            return Err("max_bytes is too small to fit the next character");
        }

        chunks.push(&s[start..end]);
        start = end;
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_safe_utf8_basic() {
        let text = "Hello, 世界! This is a test string.";
        let chunks = split_safe_utf8(text, 10).expect("Failed to split");
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], "Hello, 世");
        assert_eq!(chunks[1], "界! This ");
        assert_eq!(chunks[2], "is a test ");
        assert_eq!(chunks[3], "string.");
    }

    #[test]
    fn test_split_safe_utf8_only_multibyte() {
        let text = "こんにちは世界"; // "Hello World" in Japanese
        let chunks = split_safe_utf8(text, 9).expect("Failed to split");
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "こんに");
        assert_eq!(chunks[1], "ちは世");
        assert_eq!(chunks[2], "界");
    }

    #[test]
    fn test_split_safe_utf8_max_bytes_too_small() {
        let text = "Hello, 世界";
        let result = split_safe_utf8(text, 1);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "max_bytes is too small to fit the next character"
        );
    }

    #[test]
    fn test_split_safe_utf8_exact_boundaries() {
        let text = "Hello, 世界!";
        // "Hello, " is 7 bytes, "世" is 3 bytes, "界" is 3 bytes, "!" is 1 byte
        let chunks = split_safe_utf8(text, 7).expect("Failed to split");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Hello, ");
        assert_eq!(chunks[1], "世界!");
    }

    #[test]
    fn test_split_safe_utf8_zero_max_bytes() {
        let text = "Hello";
        let result = split_safe_utf8(text, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "max_bytes must be greater than zero");
    }
}
