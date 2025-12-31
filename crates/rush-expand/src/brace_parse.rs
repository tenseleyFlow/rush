use rush_parser::ast::{BraceExpansion, Word, WordPart};

/// Detect and parse brace patterns in a literal string
/// Returns a new Word with BraceExpansion parts if found
pub fn detect_brace_patterns(word: &Word) -> Word {
    let mut new_parts = Vec::new();

    for part in &word.parts {
        match part {
            WordPart::Literal(s) => {
                // First check for arithmetic patterns $((expr))
                if let Some(arith_word) = try_parse_arithmetic_literal(s) {
                    new_parts.extend(arith_word.parts);
                } else if let Some(brace_word) = try_parse_brace_literal(s) {
                    // Then check for brace patterns
                    new_parts.extend(brace_word.parts);
                } else {
                    new_parts.push(part.clone());
                }
            }
            _ => {
                // Keep other parts as-is
                new_parts.push(part.clone());
            }
        }
    }

    Word::new(new_parts)
}

/// Try to parse a literal string for arithmetic patterns $((expr))
fn try_parse_arithmetic_literal(s: &str) -> Option<Word> {
    // Find the pattern $((
    let start = s.find("$((")?;

    // Find the matching ))
    let rest = &s[start + 3..];
    let mut paren_count = 1;
    let mut end = None;

    for (i, ch) in rest.chars().enumerate() {
        match ch {
            '(' => paren_count += 1,
            ')' => {
                paren_count -= 1;
                if paren_count == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let end_pos = end?;

    // Extract the expression
    let before = &s[..start];
    let expr = &rest[..end_pos];
    let after = &s[start + 3 + end_pos + 2..];

    // Build the word
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(WordPart::Literal(before.to_string()));
    }
    parts.push(WordPart::ArithmeticExpansion(expr.to_string()));
    if !after.is_empty() {
        // Recursively handle remaining patterns
        if let Some(after_word) = try_parse_arithmetic_literal(after) {
            parts.extend(after_word.parts);
        } else if let Some(after_word) = try_parse_brace_literal(after) {
            parts.extend(after_word.parts);
        } else {
            parts.push(WordPart::Literal(after.to_string()));
        }
    }

    Some(Word::new(parts))
}

/// Try to parse a literal string for brace patterns
/// Returns None if no braces found, or a Word with BraceExpansion if found
fn try_parse_brace_literal(s: &str) -> Option<Word> {
    // Find the first {
    let start = s.find('{')?;
    let end = s[start..].find('}')? + start;

    // Extract the brace content
    let before = &s[..start];
    let content = &s[start + 1..end];
    let after = &s[end + 1..];

    // Try to parse the content as a brace expansion
    let brace_exp = parse_brace_content(content)?;

    // Build the word
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(WordPart::Literal(before.to_string()));
    }
    parts.push(WordPart::BraceExpansion(brace_exp));
    if !after.is_empty() {
        // Recursively handle remaining braces
        if let Some(after_word) = try_parse_brace_literal(after) {
            parts.extend(after_word.parts);
        } else {
            parts.push(WordPart::Literal(after.to_string()));
        }
    }

    Some(Word::new(parts))
}

/// Parse the content inside braces
fn parse_brace_content(content: &str) -> Option<BraceExpansion> {
    // Check if it's a sequence pattern: start..end or start..end..increment
    if content.contains("..") {
        parse_sequence(content)
    } else {
        // It's a list pattern: a,b,c
        parse_list(content)
    }
}

/// Parse a sequence pattern: 1..5 or a..z
fn parse_sequence(content: &str) -> Option<BraceExpansion> {
    let parts: Vec<&str> = content.split("..").collect();

    match parts.len() {
        2 => {
            // start..end
            Some(BraceExpansion::Sequence {
                start: parts[0].to_string(),
                end: parts[1].to_string(),
                increment: None,
            })
        }
        3 => {
            // start..end..increment
            let increment = parts[2].parse::<i32>().ok()?;
            Some(BraceExpansion::Sequence {
                start: parts[0].to_string(),
                end: parts[1].to_string(),
                increment: Some(increment),
            })
        }
        _ => None,
    }
}

/// Parse a list pattern: a,b,c
fn parse_list(content: &str) -> Option<BraceExpansion> {
    // Split by comma
    let items: Vec<String> = content.split(',').map(|s| s.to_string()).collect();

    // Need at least 2 items for a valid list
    if items.len() >= 2 {
        Some(BraceExpansion::List(items))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_list_pattern() {
        let word = Word::from_literal("file{a,b,c}.txt");
        let result = detect_brace_patterns(&word);

        assert_eq!(result.parts.len(), 3);
        assert!(matches!(result.parts[0], WordPart::Literal(_)));
        assert!(matches!(result.parts[1], WordPart::BraceExpansion(_)));
        assert!(matches!(result.parts[2], WordPart::Literal(_)));
    }

    #[test]
    fn test_detect_sequence_pattern() {
        let word = Word::from_literal("num{1..5}");
        let result = detect_brace_patterns(&word);

        assert_eq!(result.parts.len(), 2);
        assert!(matches!(result.parts[1], WordPart::BraceExpansion(_)));
    }

    #[test]
    fn test_no_brace_pattern() {
        let word = Word::from_literal("simple.txt");
        let result = detect_brace_patterns(&word);

        assert_eq!(result.parts.len(), 1);
        assert!(matches!(result.parts[0], WordPart::Literal(_)));
    }
}
