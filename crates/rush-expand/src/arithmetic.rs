use crate::context::Context;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArithmeticError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid number: {0}")]
    InvalidNumber(String),
}

/// Evaluate an arithmetic expression
/// Supports: Full bash-compatible operators with proper precedence
pub fn evaluate_arithmetic(expr: &str, context: &mut Context) -> Result<i64, ArithmeticError> {
    let tokens = tokenize(expr)?;
    let mut parser = Parser::new(tokens, context);
    parser.parse_expression()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Variable(String),

    // Arithmetic operators
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,              // **

    // Assignment operators
    Assign,             // =
    PlusAssign,         // +=
    MinusAssign,        // -=
    MultiplyAssign,     // *=
    DivideAssign,       // /=
    ModuloAssign,       // %=
    PowerAssign,        // **=

    // Bitwise operators
    BitwiseAnd,         // &
    BitwiseOr,          // |
    BitwiseXor,         // ^
    BitwiseNot,         // ~
    LeftShift,          // <<
    RightShift,         // >>

    // Comparison operators
    Less,               // <
    Greater,            // >
    LessEqual,          // <=
    GreaterEqual,       // >=
    Equal,              // ==
    NotEqual,           // !=

    // Logical operators
    LogicalAnd,         // &&
    LogicalOr,          // ||
    LogicalNot,         // !

    // Increment/Decrement
    Increment,          // ++
    Decrement,          // --

    // Ternary operator
    Question,           // ?
    Colon,              // :

    // Parentheses
    LeftParen,
    RightParen,
}

/// Tokenize an arithmetic expression with full bash operator support
fn tokenize(expr: &str) -> Result<Vec<Token>, ArithmeticError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '+' => {
                chars.next();
                if chars.peek() == Some(&'+') {
                    chars.next();
                    tokens.push(Token::Increment);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::PlusAssign);
                } else {
                    tokens.push(Token::Plus);
                }
            }
            '-' => {
                chars.next();
                if chars.peek() == Some(&'-') {
                    chars.next();
                    tokens.push(Token::Decrement);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::MinusAssign);
                } else {
                    // Check if it's a unary minus for a negative number
                    if tokens.is_empty() || is_unary_context(tokens.last()) {
                        // It's a unary minus
                        tokens.push(Token::Minus);
                    } else {
                        tokens.push(Token::Minus);
                    }
                }
            }
            '*' => {
                chars.next();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    // Could be ** or **=
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token::PowerAssign);
                    } else {
                        tokens.push(Token::Power);
                    }
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::MultiplyAssign);
                } else {
                    tokens.push(Token::Multiply);
                }
            }
            '/' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::DivideAssign);
                } else {
                    tokens.push(Token::Divide);
                }
            }
            '%' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::ModuloAssign);
                } else {
                    tokens.push(Token::Modulo);
                }
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token::LogicalAnd);
                } else {
                    tokens.push(Token::BitwiseAnd);
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::LogicalOr);
                } else {
                    tokens.push(Token::BitwiseOr);
                }
            }
            '^' => {
                chars.next();
                tokens.push(Token::BitwiseXor);
            }
            '~' => {
                chars.next();
                tokens.push(Token::BitwiseNot);
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'<') {
                    chars.next();
                    tokens.push(Token::LeftShift);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::LessEqual);
                } else {
                    tokens.push(Token::Less);
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(Token::RightShift);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::Greater);
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Equal);
                } else {
                    tokens.push(Token::Assign);
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::NotEqual);
                } else {
                    tokens.push(Token::LogicalNot);
                }
            }
            '?' => {
                chars.next();
                tokens.push(Token::Question);
            }
            ':' => {
                chars.next();
                tokens.push(Token::Colon);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LeftParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RightParen);
            }
            '0'..='9' => {
                let num = parse_number(&mut chars)?;
                tokens.push(Token::Number(num));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let var = parse_variable(&mut chars);
                tokens.push(Token::Variable(var));
            }
            _ => {
                return Err(ArithmeticError::ParseError(format!("Unexpected character: {}", ch)));
            }
        }
    }

    Ok(tokens)
}

/// Check if the previous token indicates a unary context (for unary minus/plus)
fn is_unary_context(token: Option<&Token>) -> bool {
    matches!(
        token,
        None | Some(Token::LeftParen)
            | Some(Token::Plus)
            | Some(Token::Minus)
            | Some(Token::Multiply)
            | Some(Token::Divide)
            | Some(Token::Modulo)
            | Some(Token::Power)
            | Some(Token::Assign)
            | Some(Token::PlusAssign)
            | Some(Token::MinusAssign)
            | Some(Token::MultiplyAssign)
            | Some(Token::DivideAssign)
            | Some(Token::ModuloAssign)
            | Some(Token::PowerAssign)
            | Some(Token::BitwiseAnd)
            | Some(Token::BitwiseOr)
            | Some(Token::BitwiseXor)
            | Some(Token::LeftShift)
            | Some(Token::RightShift)
            | Some(Token::Less)
            | Some(Token::Greater)
            | Some(Token::LessEqual)
            | Some(Token::GreaterEqual)
            | Some(Token::Equal)
            | Some(Token::NotEqual)
            | Some(Token::LogicalAnd)
            | Some(Token::LogicalOr)
            | Some(Token::LogicalNot)
            | Some(Token::Question)
            | Some(Token::Colon)
    )
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<i64, ArithmeticError> {
    let mut num_str = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    num_str.parse::<i64>()
        .map_err(|_| ArithmeticError::InvalidNumber(num_str))
}

fn parse_variable(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut var_name = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_alphanumeric() || ch == '_' {
            var_name.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    var_name
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    context: &'a mut Context,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token>, context: &'a mut Context) -> Self {
        Self { tokens, pos: 0, context }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    /// Entry point: parse full expression
    fn parse_expression(&mut self) -> Result<i64, ArithmeticError> {
        self.parse_ternary()
    }

    /// Level 1: Ternary conditional (? :) - lowest precedence
    fn parse_ternary(&mut self) -> Result<i64, ArithmeticError> {
        let condition = self.parse_assignment()?;

        if matches!(self.current(), Some(Token::Question)) {
            self.advance();
            let true_value = self.parse_assignment()?;
            if !matches!(self.current(), Some(Token::Colon)) {
                return Err(ArithmeticError::ParseError("Expected ':' in ternary operator".to_string()));
            }
            self.advance();
            let false_value = self.parse_ternary()?;
            Ok(if condition != 0 { true_value } else { false_value })
        } else {
            Ok(condition)
        }
    }

    /// Level 2: Assignment operators (=, +=, -=, etc.) - right associative
    fn parse_assignment(&mut self) -> Result<i64, ArithmeticError> {
        let left = self.parse_logical_or()?;

        // Check for assignment operators
        if let Some(token) = self.current() {
            let op = match token {
                Token::Assign => Some("="),
                Token::PlusAssign => Some("+="),
                Token::MinusAssign => Some("-="),
                Token::MultiplyAssign => Some("*="),
                Token::DivideAssign => Some("/="),
                Token::ModuloAssign => Some("%="),
                Token::PowerAssign => Some("**="),
                _ => None,
            };

            if let Some(op_str) = op {
                self.advance();

                // Get the variable name from previous token (must be a variable)
                // Since we already evaluated left, we need to track the variable name
                // For now, assignments require direct variable syntax
                // This is a simplification - full bash allows lvalues like a[i]

                // For assignment, we need the right side value
                let right = self.parse_assignment()?; // Right associative

                // Apply the assignment
                // Note: This is a simplified version that doesn't handle lvalues properly
                // A full implementation would need to track lvalue expressions
                let result = match op_str {
                    "=" => right,
                    "+=" => left + right,
                    "-=" => left - right,
                    "*=" => left * right,
                    "/=" => {
                        if right == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        left / right
                    }
                    "%=" => {
                        if right == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        left % right
                    }
                    "**=" => left.pow(right.max(0) as u32),
                    _ => unreachable!(),
                };

                // TODO: Store result back to variable (requires lvalue tracking)
                return Ok(result);
            }
        }

        Ok(left)
    }

    /// Level 3: Logical OR (||)
    fn parse_logical_or(&mut self) -> Result<i64, ArithmeticError> {
        let mut left = self.parse_logical_and()?;

        while matches!(self.current(), Some(Token::LogicalOr)) {
            self.advance();
            // Short-circuit: if left is true (non-zero), don't evaluate right
            if left != 0 {
                // Still need to parse right side but ignore result
                let _ = self.parse_logical_and()?;
                left = 1;
            } else {
                let right = self.parse_logical_and()?;
                left = if right != 0 { 1 } else { 0 };
            }
        }

        Ok(left)
    }

    /// Level 4: Logical AND (&&)
    fn parse_logical_and(&mut self) -> Result<i64, ArithmeticError> {
        let mut left = self.parse_bitwise_or()?;

        while matches!(self.current(), Some(Token::LogicalAnd)) {
            self.advance();
            // Short-circuit: if left is false (zero), don't evaluate right
            if left == 0 {
                // Still need to parse right side but ignore result
                let _ = self.parse_bitwise_or()?;
                left = 0;
            } else {
                let right = self.parse_bitwise_or()?;
                left = if right != 0 { 1 } else { 0 };
            }
        }

        Ok(left)
    }

    /// Level 5: Bitwise OR (|)
    fn parse_bitwise_or(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_bitwise_xor()?;

        while matches!(self.current(), Some(Token::BitwiseOr)) {
            self.advance();
            result |= self.parse_bitwise_xor()?;
        }

        Ok(result)
    }

    /// Level 6: Bitwise XOR (^)
    fn parse_bitwise_xor(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_bitwise_and()?;

        while matches!(self.current(), Some(Token::BitwiseXor)) {
            self.advance();
            result ^= self.parse_bitwise_and()?;
        }

        Ok(result)
    }

    /// Level 7: Bitwise AND (&)
    fn parse_bitwise_and(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_equality()?;

        while matches!(self.current(), Some(Token::BitwiseAnd)) {
            self.advance();
            result &= self.parse_equality()?;
        }

        Ok(result)
    }

    /// Level 8: Equality (==, !=)
    fn parse_equality(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_relational()?;

        while let Some(token) = self.current() {
            match token {
                Token::Equal => {
                    self.advance();
                    let right = self.parse_relational()?;
                    result = if result == right { 1 } else { 0 };
                }
                Token::NotEqual => {
                    self.advance();
                    let right = self.parse_relational()?;
                    result = if result != right { 1 } else { 0 };
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Level 9: Relational (<, >, <=, >=)
    fn parse_relational(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_shift()?;

        while let Some(token) = self.current() {
            match token {
                Token::Less => {
                    self.advance();
                    let right = self.parse_shift()?;
                    result = if result < right { 1 } else { 0 };
                }
                Token::Greater => {
                    self.advance();
                    let right = self.parse_shift()?;
                    result = if result > right { 1 } else { 0 };
                }
                Token::LessEqual => {
                    self.advance();
                    let right = self.parse_shift()?;
                    result = if result <= right { 1 } else { 0 };
                }
                Token::GreaterEqual => {
                    self.advance();
                    let right = self.parse_shift()?;
                    result = if result >= right { 1 } else { 0 };
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Level 10: Bitwise shift (<<, >>)
    fn parse_shift(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_additive()?;

        while let Some(token) = self.current() {
            match token {
                Token::LeftShift => {
                    self.advance();
                    let right = self.parse_additive()?;
                    result = result.wrapping_shl(right.max(0) as u32);
                }
                Token::RightShift => {
                    self.advance();
                    let right = self.parse_additive()?;
                    result = result.wrapping_shr(right.max(0) as u32);
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Level 11: Addition and subtraction (+, -)
    fn parse_additive(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_multiplicative()?;

        while let Some(token) = self.current() {
            match token {
                Token::Plus => {
                    self.advance();
                    result += self.parse_multiplicative()?;
                }
                Token::Minus => {
                    self.advance();
                    result -= self.parse_multiplicative()?;
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Level 12: Multiplication, division, modulo (*, /, %)
    fn parse_multiplicative(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_power()?;

        while let Some(token) = self.current() {
            match token {
                Token::Multiply => {
                    self.advance();
                    result *= self.parse_power()?;
                }
                Token::Divide => {
                    self.advance();
                    let divisor = self.parse_power()?;
                    if divisor == 0 {
                        return Err(ArithmeticError::DivisionByZero);
                    }
                    result /= divisor;
                }
                Token::Modulo => {
                    self.advance();
                    let divisor = self.parse_power()?;
                    if divisor == 0 {
                        return Err(ArithmeticError::DivisionByZero);
                    }
                    result %= divisor;
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Level 13: Power (**) - right associative
    fn parse_power(&mut self) -> Result<i64, ArithmeticError> {
        let base = self.parse_unary()?;

        if matches!(self.current(), Some(Token::Power)) {
            self.advance();
            let exponent = self.parse_power()?; // Right associative
            Ok(base.pow(exponent.max(0) as u32))
        } else {
            Ok(base)
        }
    }

    /// Level 14: Unary operators (!, ~, unary +, unary -)
    fn parse_unary(&mut self) -> Result<i64, ArithmeticError> {
        match self.current() {
            Some(Token::LogicalNot) => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(if operand == 0 { 1 } else { 0 })
            }
            Some(Token::BitwiseNot) => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(!operand)
            }
            Some(Token::Plus) => {
                self.advance();
                self.parse_unary() // Unary plus doesn't change value
            }
            Some(Token::Minus) => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(-operand)
            }
            Some(Token::Increment) => {
                // Pre-increment
                self.advance();
                // TODO: Implement proper pre-increment with lvalue
                let value = self.parse_postfix()?;
                Ok(value + 1)
            }
            Some(Token::Decrement) => {
                // Pre-decrement
                self.advance();
                // TODO: Implement proper pre-decrement with lvalue
                let value = self.parse_postfix()?;
                Ok(value - 1)
            }
            _ => self.parse_postfix(),
        }
    }

    /// Level 15: Postfix operators (++, --)
    fn parse_postfix(&mut self) -> Result<i64, ArithmeticError> {
        let result = self.parse_primary()?;

        // Check for postfix operators
        match self.current() {
            Some(Token::Increment) => {
                self.advance();
                // TODO: Implement proper post-increment with lvalue
                // For now, return current value (post-increment returns old value)
                Ok(result)
            }
            Some(Token::Decrement) => {
                self.advance();
                // TODO: Implement proper post-decrement with lvalue
                // For now, return current value (post-decrement returns old value)
                Ok(result)
            }
            _ => Ok(result),
        }
    }

    /// Level 16: Primary expressions (numbers, variables, parenthesized expressions)
    fn parse_primary(&mut self) -> Result<i64, ArithmeticError> {
        match self.current() {
            Some(Token::Number(n)) => {
                let value = *n;
                self.advance();
                Ok(value)
            }
            Some(Token::Variable(name)) => {
                let name = name.clone();
                self.advance();
                // Look up variable value
                let value = self.context.get_var(&name)
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0);
                Ok(value)
            }
            Some(Token::LeftParen) => {
                self.advance();
                let result = self.parse_expression()?;
                if !matches!(self.current(), Some(Token::RightParen)) {
                    return Err(ArithmeticError::ParseError("Expected ')'".to_string()));
                }
                self.advance();
                Ok(result)
            }
            _ => Err(ArithmeticError::ParseError("Unexpected token".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_addition() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("2 + 3", &mut ctx).unwrap(), 5);
    }

    #[test]
    fn test_multiplication() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("4 * 5", &mut ctx).unwrap(), 20);
    }

    #[test]
    fn test_complex_expression() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("(2 + 3) * 4", &mut ctx).unwrap(), 20);
    }

    #[test]
    fn test_with_variable() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x * 2", &mut ctx).unwrap(), 20);
    }

    #[test]
    fn test_division() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("10 / 2", &mut ctx).unwrap(), 5);
    }

    #[test]
    fn test_modulo() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("10 % 3", &mut ctx).unwrap(), 1);
    }

    #[test]
    fn test_negative_numbers() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("-5 + 3", &mut ctx).unwrap(), -2);
    }

    #[test]
    fn test_precedence() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("2 + 3 * 4", &mut ctx).unwrap(), 14);
    }
}
