use std::fs;
use std::path::Path;

/// Execute the test builtin ([  or test)
///
/// Returns true (0) if the test succeeds, false (1) if it fails
pub fn execute_test(args: &[String]) -> i32 {
    // Handle [ command which requires trailing ]
    let args = if args.last().map(|s| s.as_str()) == Some("]") {
        &args[..args.len() - 1]
    } else {
        args
    };

    if args.is_empty() {
        return 1; // Empty test is false
    }

    match evaluate_test(args) {
        Ok(result) => if result { 0 } else { 1 },
        Err(_) => 2, // Syntax error
    }
}

fn evaluate_test(args: &[String]) -> Result<bool, String> {
    if args.is_empty() {
        return Ok(false);
    }

    // Handle negation
    if args[0] == "!" {
        return Ok(!evaluate_test(&args[1..])?);
    }

    // Single argument: non-empty string test
    if args.len() == 1 {
        return Ok(!args[0].is_empty());
    }

    // Two arguments: unary operators
    if args.len() == 2 {
        return evaluate_unary(&args[0], &args[1]);
    }

    // Three arguments: binary operators
    if args.len() == 3 {
        return evaluate_binary(&args[0], &args[1], &args[2]);
    }

    // Four arguments with negation
    if args.len() == 4 && args[0] == "!" {
        return Ok(!evaluate_binary(&args[1], &args[2], &args[3])?);
    }

    // Complex expressions with -a (and) or -o (or)
    // Find the operator (rightmost for left-to-right evaluation)
    for (i, arg) in args.iter().enumerate() {
        if arg == "-o" && i > 0 && i < args.len() - 1 {
            let left = evaluate_test(&args[..i])?;
            let right = evaluate_test(&args[i + 1..])?;
            return Ok(left || right);
        }
    }

    for (i, arg) in args.iter().enumerate() {
        if arg == "-a" && i > 0 && i < args.len() - 1 {
            let left = evaluate_test(&args[..i])?;
            let right = evaluate_test(&args[i + 1..])?;
            return Ok(left && right);
        }
    }

    Err("Invalid test syntax".to_string())
}

fn evaluate_unary(op: &str, arg: &str) -> Result<bool, String> {
    match op {
        // String tests
        "-z" => Ok(arg.is_empty()),
        "-n" => Ok(!arg.is_empty()),

        // File tests
        "-e" => Ok(Path::new(arg).exists()),
        "-f" => Ok(Path::new(arg).is_file()),
        "-d" => Ok(Path::new(arg).is_dir()),
        "-r" => Ok(is_readable(arg)),
        "-w" => Ok(is_writable(arg)),
        "-x" => Ok(is_executable(arg)),
        "-s" => Ok(file_has_size(arg)),

        _ => Err(format!("Unknown unary operator: {}", op)),
    }
}

fn evaluate_binary(left: &str, op: &str, right: &str) -> Result<bool, String> {
    match op {
        // String comparisons
        "=" | "==" => Ok(left == right),
        "!=" => Ok(left != right),

        // Numeric comparisons
        "-eq" => compare_numbers(left, right, |a, b| a == b),
        "-ne" => compare_numbers(left, right, |a, b| a != b),
        "-lt" => compare_numbers(left, right, |a, b| a < b),
        "-le" => compare_numbers(left, right, |a, b| a <= b),
        "-gt" => compare_numbers(left, right, |a, b| a > b),
        "-ge" => compare_numbers(left, right, |a, b| a >= b),

        _ => Err(format!("Unknown binary operator: {}", op)),
    }
}

fn compare_numbers<F>(left: &str, right: &str, compare: F) -> Result<bool, String>
where
    F: FnOnce(i64, i64) -> bool,
{
    let left_num = left.parse::<i64>()
        .map_err(|_| format!("Not a number: {}", left))?;
    let right_num = right.parse::<i64>()
        .map_err(|_| format!("Not a number: {}", right))?;

    Ok(compare(left_num, right_num))
}

fn is_readable(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

fn is_writable(path: &str) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        !metadata.permissions().readonly()
    } else {
        false
    }
}

#[cfg(unix)]
fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        metadata.permissions().mode() & 0o111 != 0
    } else {
        false
    }
}

#[cfg(not(unix))]
fn is_executable(_path: &str) -> bool {
    // On non-Unix, we can't easily check execute permissions
    Path::new(_path).exists()
}

fn file_has_size(path: &str) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len() > 0
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_equal() {
        assert_eq!(execute_test(&["hello".to_string(), "=".to_string(), "hello".to_string()]), 0);
        assert_eq!(execute_test(&["hello".to_string(), "=".to_string(), "world".to_string()]), 1);
    }

    #[test]
    fn test_string_not_equal() {
        assert_eq!(execute_test(&["hello".to_string(), "!=".to_string(), "world".to_string()]), 0);
        assert_eq!(execute_test(&["hello".to_string(), "!=".to_string(), "hello".to_string()]), 1);
    }

    #[test]
    fn test_string_empty() {
        assert_eq!(execute_test(&["-z".to_string(), "".to_string()]), 0);
        assert_eq!(execute_test(&["-z".to_string(), "hello".to_string()]), 1);
        assert_eq!(execute_test(&["-n".to_string(), "hello".to_string()]), 0);
        assert_eq!(execute_test(&["-n".to_string(), "".to_string()]), 1);
    }

    #[test]
    fn test_numeric_comparison() {
        assert_eq!(execute_test(&["5".to_string(), "-eq".to_string(), "5".to_string()]), 0);
        assert_eq!(execute_test(&["5".to_string(), "-eq".to_string(), "6".to_string()]), 1);
        assert_eq!(execute_test(&["3".to_string(), "-lt".to_string(), "5".to_string()]), 0);
        assert_eq!(execute_test(&["5".to_string(), "-lt".to_string(), "3".to_string()]), 1);
        assert_eq!(execute_test(&["7".to_string(), "-gt".to_string(), "5".to_string()]), 0);
    }

    #[test]
    fn test_negation() {
        assert_eq!(execute_test(&["!".to_string(), "-z".to_string(), "hello".to_string()]), 0);
        assert_eq!(execute_test(&["!".to_string(), "-z".to_string(), "".to_string()]), 1);
    }
}
