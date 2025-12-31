use rush_parser::ast::BraceExpansion;

/// Expand a brace expansion pattern into a list of strings
pub fn expand_brace(brace: &BraceExpansion) -> Vec<String> {
    match brace {
        BraceExpansion::List(items) => items.clone(),
        BraceExpansion::Sequence { start, end, increment } => {
            expand_sequence(start, end, increment.unwrap_or(1))
        }
    }
}

/// Expand a sequence like {1..10} or {a..z}
fn expand_sequence(start: &str, end: &str, increment: i32) -> Vec<String> {
    // Try numeric sequence first
    if let (Ok(start_num), Ok(end_num)) = (start.parse::<i32>(), end.parse::<i32>()) {
        expand_numeric_sequence(start_num, end_num, increment, start.len())
    } else if start.len() == 1 && end.len() == 1 {
        // Try character sequence
        let start_char = start.chars().next().unwrap();
        let end_char = end.chars().next().unwrap();
        expand_char_sequence(start_char, end_char, increment)
    } else {
        // Invalid sequence, return empty
        vec![]
    }
}

/// Expand a numeric sequence: {1..10} or {01..05}
fn expand_numeric_sequence(start: i32, end: i32, increment: i32, width: usize) -> Vec<String> {
    let mut result = Vec::new();

    if increment == 0 {
        return result;
    }

    // Check if we should use zero-padding
    let use_padding = width > 1;

    if start <= end && increment > 0 {
        // Ascending
        let mut current = start;
        while current <= end {
            if use_padding {
                result.push(format!("{:0width$}", current, width = width));
            } else {
                result.push(current.to_string());
            }
            current += increment;
        }
    } else if start >= end && increment < 0 {
        // Descending
        let mut current = start;
        while current >= end {
            if use_padding {
                result.push(format!("{:0width$}", current, width = width));
            } else {
                result.push(current.to_string());
            }
            current += increment;
        }
    } else if start > end && increment > 0 {
        // Descending with positive increment (use negative)
        let mut current = start;
        while current >= end {
            if use_padding {
                result.push(format!("{:0width$}", current, width = width));
            } else {
                result.push(current.to_string());
            }
            current -= increment;
        }
    }

    result
}

/// Expand a character sequence: {a..z} or {Z..A}
fn expand_char_sequence(start: char, end: char, increment: i32) -> Vec<String> {
    let mut result = Vec::new();

    if increment == 0 {
        return result;
    }

    let start_code = start as i32;
    let end_code = end as i32;

    if start_code <= end_code && increment > 0 {
        // Ascending
        let mut current = start_code;
        while current <= end_code {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(ch.to_string());
            }
            current += increment;
        }
    } else if start_code >= end_code && increment < 0 {
        // Descending
        let mut current = start_code;
        while current >= end_code {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(ch.to_string());
            }
            current += increment;
        }
    } else if start_code > end_code && increment > 0 {
        // Descending with positive increment
        let mut current = start_code;
        while current >= end_code {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(ch.to_string());
            }
            current -= increment;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rush_parser::ast::BraceExpansion;

    #[test]
    fn test_list_expansion() {
        let brace = BraceExpansion::List(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]);
        assert_eq!(expand_brace(&brace), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_numeric_sequence() {
        let brace = BraceExpansion::Sequence {
            start: "1".to_string(),
            end: "5".to_string(),
            increment: None,
        };
        assert_eq!(expand_brace(&brace), vec!["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn test_numeric_sequence_descending() {
        let brace = BraceExpansion::Sequence {
            start: "5".to_string(),
            end: "1".to_string(),
            increment: None,
        };
        assert_eq!(expand_brace(&brace), vec!["5", "4", "3", "2", "1"]);
    }

    #[test]
    fn test_char_sequence() {
        let brace = BraceExpansion::Sequence {
            start: "a".to_string(),
            end: "d".to_string(),
            increment: None,
        };
        assert_eq!(expand_brace(&brace), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_padded_numbers() {
        let brace = BraceExpansion::Sequence {
            start: "01".to_string(),
            end: "05".to_string(),
            increment: None,
        };
        assert_eq!(expand_brace(&brace), vec!["01", "02", "03", "04", "05"]);
    }
}
