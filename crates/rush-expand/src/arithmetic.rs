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

    #[error("Not an lvalue: cannot assign to expression")]
    NotAnLvalue,
}

/// Represents either a concrete value or an lvalue (variable that can be assigned to)
#[derive(Debug, Clone)]
enum LValue {
    /// A concrete value (result of an expression)
    Value(i64),
    /// A variable reference with its name and current value
    Variable { name: String, value: i64 },
}

impl LValue {
    /// Get the numeric value
    fn value(&self) -> i64 {
        match self {
            LValue::Value(v) => *v,
            LValue::Variable { value, .. } => *value,
        }
    }

    /// Get the variable name if this is an lvalue, None otherwise
    fn var_name(&self) -> Option<&str> {
        match self {
            LValue::Value(_) => None,
            LValue::Variable { name, .. } => Some(name),
        }
    }
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
    AndAssign,          // &=
    OrAssign,           // |=
    XorAssign,          // ^=
    LeftShiftAssign,    // <<=
    RightShiftAssign,   // >>=

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

    // Comma operator
    Comma,              // ,

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
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::AndAssign);
                } else {
                    tokens.push(Token::BitwiseAnd);
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::LogicalOr);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::OrAssign);
                } else {
                    tokens.push(Token::BitwiseOr);
                }
            }
            '^' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::XorAssign);
                } else {
                    tokens.push(Token::BitwiseXor);
                }
            }
            '~' => {
                chars.next();
                tokens.push(Token::BitwiseNot);
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'<') {
                    chars.next();
                    // Could be << or <<=
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token::LeftShiftAssign);
                    } else {
                        tokens.push(Token::LeftShift);
                    }
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
                    // Could be >> or >>=
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token::RightShiftAssign);
                    } else {
                        tokens.push(Token::RightShift);
                    }
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
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
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
            | Some(Token::Comma)
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

    /// Set a variable in the context and return the new value
    fn set_var(&mut self, name: &str, value: i64) -> i64 {
        let _ = self.context.set_var(name, value.to_string());
        value
    }

    /// Get variable value from context
    fn get_var(&self, name: &str) -> i64 {
        self.context.get_var(name)
            .unwrap_or("0")
            .parse::<i64>()
            .unwrap_or(0)
    }

    /// Entry point: parse full expression
    fn parse_expression(&mut self) -> Result<i64, ArithmeticError> {
        Ok(self.parse_comma()?.value())
    }

    /// Parse expression returning LValue (for internal use)
    fn parse_expression_lvalue(&mut self) -> Result<LValue, ArithmeticError> {
        self.parse_comma()
    }

    /// Level 0: Comma operator (,) - lowest precedence
    /// Evaluates all expressions from left to right and returns the last value
    fn parse_comma(&mut self) -> Result<LValue, ArithmeticError> {
        let mut result = self.parse_ternary()?;

        while matches!(self.current(), Some(Token::Comma)) {
            self.advance();
            // Evaluate the next expression; previous result is discarded
            result = self.parse_ternary()?;
        }

        Ok(result)
    }

    /// Level 1: Ternary conditional (? :)
    fn parse_ternary(&mut self) -> Result<LValue, ArithmeticError> {
        let condition = self.parse_assignment()?;

        if matches!(self.current(), Some(Token::Question)) {
            self.advance();
            let true_value = self.parse_assignment()?;
            if !matches!(self.current(), Some(Token::Colon)) {
                return Err(ArithmeticError::ParseError("Expected ':' in ternary operator".to_string()));
            }
            self.advance();
            let false_value = self.parse_ternary()?;
            // Ternary result is not an lvalue
            Ok(LValue::Value(if condition.value() != 0 {
                true_value.value()
            } else {
                false_value.value()
            }))
        } else {
            Ok(condition)
        }
    }

    /// Level 2: Assignment operators (=, +=, -=, etc.) - right associative
    fn parse_assignment(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_logical_or()?;

        // Check for assignment operators
        if let Some(token) = self.current().cloned() {
            let op = match &token {
                Token::Assign => Some("="),
                Token::PlusAssign => Some("+="),
                Token::MinusAssign => Some("-="),
                Token::MultiplyAssign => Some("*="),
                Token::DivideAssign => Some("/="),
                Token::ModuloAssign => Some("%="),
                Token::PowerAssign => Some("**="),
                Token::AndAssign => Some("&="),
                Token::OrAssign => Some("|="),
                Token::XorAssign => Some("^="),
                Token::LeftShiftAssign => Some("<<="),
                Token::RightShiftAssign => Some(">>="),
                _ => None,
            };

            if let Some(op_str) = op {
                // Check that left is an lvalue (variable)
                let var_name = match left.var_name() {
                    Some(name) => name.to_string(),
                    None => return Err(ArithmeticError::NotAnLvalue),
                };

                self.advance();

                // For assignment, we need the right side value (right associative)
                let right = self.parse_assignment()?.value();
                let left_val = left.value();

                // Compute the result
                let result = match op_str {
                    "=" => right,
                    "+=" => left_val + right,
                    "-=" => left_val - right,
                    "*=" => left_val * right,
                    "/=" => {
                        if right == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        left_val / right
                    }
                    "%=" => {
                        if right == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        left_val % right
                    }
                    "**=" => left_val.pow(right.max(0) as u32),
                    "&=" => left_val & right,
                    "|=" => left_val | right,
                    "^=" => left_val ^ right,
                    "<<=" => left_val.wrapping_shl(right.max(0) as u32),
                    ">>=" => left_val.wrapping_shr(right.max(0) as u32),
                    _ => unreachable!(),
                };

                // Store result back to variable
                self.set_var(&var_name, result);

                // Return as a new lvalue pointing to the same variable with updated value
                return Ok(LValue::Variable { name: var_name, value: result });
            }
        }

        Ok(left)
    }

    /// Level 3: Logical OR (||)
    fn parse_logical_or(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_logical_and()?;

        if matches!(self.current(), Some(Token::LogicalOr)) {
            let mut result = left.value();
            while matches!(self.current(), Some(Token::LogicalOr)) {
                self.advance();
                // Short-circuit: if left is true (non-zero), don't evaluate right
                if result != 0 {
                    // Still need to parse right side but ignore result
                    let _ = self.parse_logical_and()?;
                    result = 1;
                } else {
                    let right = self.parse_logical_and()?.value();
                    result = if right != 0 { 1 } else { 0 };
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 4: Logical AND (&&)
    fn parse_logical_and(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_bitwise_or()?;

        if matches!(self.current(), Some(Token::LogicalAnd)) {
            let mut result = left.value();
            while matches!(self.current(), Some(Token::LogicalAnd)) {
                self.advance();
                // Short-circuit: if left is false (zero), don't evaluate right
                if result == 0 {
                    // Still need to parse right side but ignore result
                    let _ = self.parse_bitwise_or()?;
                    result = 0;
                } else {
                    let right = self.parse_bitwise_or()?.value();
                    result = if right != 0 { 1 } else { 0 };
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 5: Bitwise OR (|)
    fn parse_bitwise_or(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_bitwise_xor()?;

        if matches!(self.current(), Some(Token::BitwiseOr)) {
            let mut result = left.value();
            while matches!(self.current(), Some(Token::BitwiseOr)) {
                self.advance();
                result |= self.parse_bitwise_xor()?.value();
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 6: Bitwise XOR (^)
    fn parse_bitwise_xor(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_bitwise_and()?;

        if matches!(self.current(), Some(Token::BitwiseXor)) {
            let mut result = left.value();
            while matches!(self.current(), Some(Token::BitwiseXor)) {
                self.advance();
                result ^= self.parse_bitwise_and()?.value();
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 7: Bitwise AND (&)
    fn parse_bitwise_and(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_equality()?;

        if matches!(self.current(), Some(Token::BitwiseAnd)) {
            let mut result = left.value();
            while matches!(self.current(), Some(Token::BitwiseAnd)) {
                self.advance();
                result &= self.parse_equality()?.value();
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 8: Equality (==, !=)
    fn parse_equality(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_relational()?;

        if matches!(self.current(), Some(Token::Equal) | Some(Token::NotEqual)) {
            let mut result = left.value();
            while let Some(token) = self.current().cloned() {
                match token {
                    Token::Equal => {
                        self.advance();
                        let right = self.parse_relational()?.value();
                        result = if result == right { 1 } else { 0 };
                    }
                    Token::NotEqual => {
                        self.advance();
                        let right = self.parse_relational()?.value();
                        result = if result != right { 1 } else { 0 };
                    }
                    _ => break,
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 9: Relational (<, >, <=, >=)
    fn parse_relational(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_shift()?;

        if matches!(self.current(), Some(Token::Less) | Some(Token::Greater) | Some(Token::LessEqual) | Some(Token::GreaterEqual)) {
            let mut result = left.value();
            while let Some(token) = self.current().cloned() {
                match token {
                    Token::Less => {
                        self.advance();
                        let right = self.parse_shift()?.value();
                        result = if result < right { 1 } else { 0 };
                    }
                    Token::Greater => {
                        self.advance();
                        let right = self.parse_shift()?.value();
                        result = if result > right { 1 } else { 0 };
                    }
                    Token::LessEqual => {
                        self.advance();
                        let right = self.parse_shift()?.value();
                        result = if result <= right { 1 } else { 0 };
                    }
                    Token::GreaterEqual => {
                        self.advance();
                        let right = self.parse_shift()?.value();
                        result = if result >= right { 1 } else { 0 };
                    }
                    _ => break,
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 10: Bitwise shift (<<, >>)
    fn parse_shift(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_additive()?;

        if matches!(self.current(), Some(Token::LeftShift) | Some(Token::RightShift)) {
            let mut result = left.value();
            while let Some(token) = self.current().cloned() {
                match token {
                    Token::LeftShift => {
                        self.advance();
                        let right = self.parse_additive()?.value();
                        result = result.wrapping_shl(right.max(0) as u32);
                    }
                    Token::RightShift => {
                        self.advance();
                        let right = self.parse_additive()?.value();
                        result = result.wrapping_shr(right.max(0) as u32);
                    }
                    _ => break,
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 11: Addition and subtraction (+, -)
    fn parse_additive(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_multiplicative()?;

        if matches!(self.current(), Some(Token::Plus) | Some(Token::Minus)) {
            let mut result = left.value();
            while let Some(token) = self.current().cloned() {
                match token {
                    Token::Plus => {
                        self.advance();
                        result += self.parse_multiplicative()?.value();
                    }
                    Token::Minus => {
                        self.advance();
                        result -= self.parse_multiplicative()?.value();
                    }
                    _ => break,
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 12: Multiplication, division, modulo (*, /, %)
    fn parse_multiplicative(&mut self) -> Result<LValue, ArithmeticError> {
        let left = self.parse_power()?;

        if matches!(self.current(), Some(Token::Multiply) | Some(Token::Divide) | Some(Token::Modulo)) {
            let mut result = left.value();
            while let Some(token) = self.current().cloned() {
                match token {
                    Token::Multiply => {
                        self.advance();
                        result *= self.parse_power()?.value();
                    }
                    Token::Divide => {
                        self.advance();
                        let divisor = self.parse_power()?.value();
                        if divisor == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        result /= divisor;
                    }
                    Token::Modulo => {
                        self.advance();
                        let divisor = self.parse_power()?.value();
                        if divisor == 0 {
                            return Err(ArithmeticError::DivisionByZero);
                        }
                        result %= divisor;
                    }
                    _ => break,
                }
            }
            Ok(LValue::Value(result))
        } else {
            Ok(left)
        }
    }

    /// Level 13: Power (**) - right associative
    fn parse_power(&mut self) -> Result<LValue, ArithmeticError> {
        let base = self.parse_unary()?;

        if matches!(self.current(), Some(Token::Power)) {
            self.advance();
            let exponent = self.parse_power()?.value(); // Right associative
            Ok(LValue::Value(base.value().pow(exponent.max(0) as u32)))
        } else {
            Ok(base)
        }
    }

    /// Level 14: Unary operators (!, ~, unary +, unary -)
    fn parse_unary(&mut self) -> Result<LValue, ArithmeticError> {
        match self.current().cloned() {
            Some(Token::LogicalNot) => {
                self.advance();
                let operand = self.parse_unary()?.value();
                Ok(LValue::Value(if operand == 0 { 1 } else { 0 }))
            }
            Some(Token::BitwiseNot) => {
                self.advance();
                let operand = self.parse_unary()?.value();
                Ok(LValue::Value(!operand))
            }
            Some(Token::Plus) => {
                self.advance();
                self.parse_unary() // Unary plus doesn't change value
            }
            Some(Token::Minus) => {
                self.advance();
                let operand = self.parse_unary()?.value();
                Ok(LValue::Value(-operand))
            }
            Some(Token::Increment) => {
                // Pre-increment: ++x returns x+1 and stores x+1
                self.advance();
                let operand = self.parse_postfix()?;
                match operand.var_name() {
                    Some(name) => {
                        let new_value = operand.value() + 1;
                        self.set_var(name, new_value);
                        Ok(LValue::Variable { name: name.to_string(), value: new_value })
                    }
                    None => Err(ArithmeticError::NotAnLvalue),
                }
            }
            Some(Token::Decrement) => {
                // Pre-decrement: --x returns x-1 and stores x-1
                self.advance();
                let operand = self.parse_postfix()?;
                match operand.var_name() {
                    Some(name) => {
                        let new_value = operand.value() - 1;
                        self.set_var(name, new_value);
                        Ok(LValue::Variable { name: name.to_string(), value: new_value })
                    }
                    None => Err(ArithmeticError::NotAnLvalue),
                }
            }
            _ => self.parse_postfix(),
        }
    }

    /// Level 15: Postfix operators (++, --)
    fn parse_postfix(&mut self) -> Result<LValue, ArithmeticError> {
        let operand = self.parse_primary()?;

        // Check for postfix operators
        match self.current() {
            Some(Token::Increment) => {
                self.advance();
                // Post-increment: x++ returns old value, stores x+1
                match operand.var_name() {
                    Some(name) => {
                        let old_value = operand.value();
                        self.set_var(name, old_value + 1);
                        // Return the old value (as a non-lvalue since it's the pre-increment value)
                        Ok(LValue::Value(old_value))
                    }
                    None => Err(ArithmeticError::NotAnLvalue),
                }
            }
            Some(Token::Decrement) => {
                self.advance();
                // Post-decrement: x-- returns old value, stores x-1
                match operand.var_name() {
                    Some(name) => {
                        let old_value = operand.value();
                        self.set_var(name, old_value - 1);
                        // Return the old value (as a non-lvalue since it's the pre-decrement value)
                        Ok(LValue::Value(old_value))
                    }
                    None => Err(ArithmeticError::NotAnLvalue),
                }
            }
            _ => Ok(operand),
        }
    }

    /// Level 16: Primary expressions (numbers, variables, parenthesized expressions)
    fn parse_primary(&mut self) -> Result<LValue, ArithmeticError> {
        match self.current().cloned() {
            Some(Token::Number(n)) => {
                self.advance();
                Ok(LValue::Value(n))
            }
            Some(Token::Variable(name)) => {
                self.advance();
                // Look up variable value
                let value = self.get_var(&name);
                Ok(LValue::Variable { name, value })
            }
            Some(Token::LeftParen) => {
                self.advance();
                let result = self.parse_expression_lvalue()?;
                if !matches!(self.current(), Some(Token::RightParen)) {
                    return Err(ArithmeticError::ParseError("Expected ')'".to_string()));
                }
                self.advance();
                // Parenthesized expressions are not lvalues (even if they contain a single variable)
                Ok(LValue::Value(result.value()))
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

    // Lvalue tests

    #[test]
    fn test_simple_assignment() {
        let mut ctx = Context::empty();
        // x = 5 should set x to 5 and return 5
        assert_eq!(evaluate_arithmetic("x = 5", &mut ctx).unwrap(), 5);
        assert_eq!(ctx.get_var("x"), Some("5"));
    }

    #[test]
    fn test_compound_assignment_add() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        // x += 3 should set x to 13 and return 13
        assert_eq!(evaluate_arithmetic("x += 3", &mut ctx).unwrap(), 13);
        assert_eq!(ctx.get_var("x"), Some("13"));
    }

    #[test]
    fn test_compound_assignment_sub() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x -= 3", &mut ctx).unwrap(), 7);
        assert_eq!(ctx.get_var("x"), Some("7"));
    }

    #[test]
    fn test_compound_assignment_mul() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x *= 3", &mut ctx).unwrap(), 30);
        assert_eq!(ctx.get_var("x"), Some("30"));
    }

    #[test]
    fn test_compound_assignment_div() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x /= 2", &mut ctx).unwrap(), 5);
        assert_eq!(ctx.get_var("x"), Some("5"));
    }

    #[test]
    fn test_compound_assignment_mod() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x %= 3", &mut ctx).unwrap(), 1);
        assert_eq!(ctx.get_var("x"), Some("1"));
    }

    #[test]
    fn test_pre_increment() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // ++x should increment x and return the new value
        assert_eq!(evaluate_arithmetic("++x", &mut ctx).unwrap(), 6);
        assert_eq!(ctx.get_var("x"), Some("6"));
    }

    #[test]
    fn test_pre_decrement() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // --x should decrement x and return the new value
        assert_eq!(evaluate_arithmetic("--x", &mut ctx).unwrap(), 4);
        assert_eq!(ctx.get_var("x"), Some("4"));
    }

    #[test]
    fn test_post_increment() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // x++ should return the old value but increment x
        assert_eq!(evaluate_arithmetic("x++", &mut ctx).unwrap(), 5);
        assert_eq!(ctx.get_var("x"), Some("6"));
    }

    #[test]
    fn test_post_decrement() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // x-- should return the old value but decrement x
        assert_eq!(evaluate_arithmetic("x--", &mut ctx).unwrap(), 5);
        assert_eq!(ctx.get_var("x"), Some("4"));
    }

    #[test]
    fn test_chained_assignment() {
        let mut ctx = Context::empty();
        // a = b = c = 5 should set all three to 5
        assert_eq!(evaluate_arithmetic("a = b = c = 5", &mut ctx).unwrap(), 5);
        assert_eq!(ctx.get_var("a"), Some("5"));
        assert_eq!(ctx.get_var("b"), Some("5"));
        assert_eq!(ctx.get_var("c"), Some("5"));
    }

    #[test]
    fn test_increment_in_expression() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // 2 + ++x should be 2 + 6 = 8
        assert_eq!(evaluate_arithmetic("2 + ++x", &mut ctx).unwrap(), 8);
        assert_eq!(ctx.get_var("x"), Some("6"));
    }

    #[test]
    fn test_post_increment_in_expression() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap();
        // 2 + x++ should be 2 + 5 = 7 (x becomes 6 after)
        assert_eq!(evaluate_arithmetic("2 + x++", &mut ctx).unwrap(), 7);
        assert_eq!(ctx.get_var("x"), Some("6"));
    }

    #[test]
    fn test_assignment_not_lvalue_error() {
        let mut ctx = Context::empty();
        // (x) is not an lvalue, so (x) = 5 should fail
        let result = evaluate_arithmetic("(x) = 5", &mut ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_increment_not_lvalue_error() {
        let mut ctx = Context::empty();
        // ++5 should fail (5 is not an lvalue)
        let result = evaluate_arithmetic("++5", &mut ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_power_assignment() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "2").unwrap();
        assert_eq!(evaluate_arithmetic("x **= 3", &mut ctx).unwrap(), 8);
        assert_eq!(ctx.get_var("x"), Some("8"));
    }

    #[test]
    fn test_ternary_operator() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("1 ? 10 : 20", &mut ctx).unwrap(), 10);
        assert_eq!(evaluate_arithmetic("0 ? 10 : 20", &mut ctx).unwrap(), 20);
    }

    #[test]
    fn test_comparison_operators() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("5 < 10", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("10 < 5", &mut ctx).unwrap(), 0);
        assert_eq!(evaluate_arithmetic("5 == 5", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("5 != 5", &mut ctx).unwrap(), 0);
        assert_eq!(evaluate_arithmetic("5 <= 5", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("5 >= 5", &mut ctx).unwrap(), 1);
    }

    #[test]
    fn test_logical_operators() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("1 && 1", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("1 && 0", &mut ctx).unwrap(), 0);
        assert_eq!(evaluate_arithmetic("0 || 1", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("0 || 0", &mut ctx).unwrap(), 0);
        assert_eq!(evaluate_arithmetic("!0", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("!1", &mut ctx).unwrap(), 0);
    }

    #[test]
    fn test_bitwise_operators() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("5 & 3", &mut ctx).unwrap(), 1);
        assert_eq!(evaluate_arithmetic("5 | 3", &mut ctx).unwrap(), 7);
        assert_eq!(evaluate_arithmetic("5 ^ 3", &mut ctx).unwrap(), 6);
        assert_eq!(evaluate_arithmetic("~0", &mut ctx).unwrap(), -1);
        assert_eq!(evaluate_arithmetic("1 << 4", &mut ctx).unwrap(), 16);
        assert_eq!(evaluate_arithmetic("16 >> 2", &mut ctx).unwrap(), 4);
    }

    #[test]
    fn test_bitwise_assignment_and() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "7").unwrap(); // 0b111
        assert_eq!(evaluate_arithmetic("x &= 3", &mut ctx).unwrap(), 3); // 0b111 & 0b011 = 0b011
        assert_eq!(ctx.get_var("x"), Some("3"));
    }

    #[test]
    fn test_bitwise_assignment_or() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "5").unwrap(); // 0b101
        assert_eq!(evaluate_arithmetic("x |= 2", &mut ctx).unwrap(), 7); // 0b101 | 0b010 = 0b111
        assert_eq!(ctx.get_var("x"), Some("7"));
    }

    #[test]
    fn test_bitwise_assignment_xor() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "7").unwrap(); // 0b111
        assert_eq!(evaluate_arithmetic("x ^= 3", &mut ctx).unwrap(), 4); // 0b111 ^ 0b011 = 0b100
        assert_eq!(ctx.get_var("x"), Some("4"));
    }

    #[test]
    fn test_bitwise_assignment_left_shift() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "1").unwrap();
        assert_eq!(evaluate_arithmetic("x <<= 4", &mut ctx).unwrap(), 16);
        assert_eq!(ctx.get_var("x"), Some("16"));
    }

    #[test]
    fn test_bitwise_assignment_right_shift() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "16").unwrap();
        assert_eq!(evaluate_arithmetic("x >>= 2", &mut ctx).unwrap(), 4);
        assert_eq!(ctx.get_var("x"), Some("4"));
    }

    #[test]
    fn test_power_operator() {
        let mut ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("2 ** 10", &mut ctx).unwrap(), 1024);
        // Right associative: 2 ** 3 ** 2 = 2 ** 9 = 512
        assert_eq!(evaluate_arithmetic("2 ** 3 ** 2", &mut ctx).unwrap(), 512);
    }

    // Comma operator tests

    #[test]
    fn test_comma_operator_simple() {
        let mut ctx = Context::empty();
        // Comma operator evaluates left to right, returns last value
        assert_eq!(evaluate_arithmetic("1, 2, 3", &mut ctx).unwrap(), 3);
    }

    #[test]
    fn test_comma_operator_with_assignments() {
        let mut ctx = Context::empty();
        // (a=1, b=2, a+b) sets a and b, returns sum
        assert_eq!(evaluate_arithmetic("a=1, b=2, a+b", &mut ctx).unwrap(), 3);
        assert_eq!(ctx.get_var("a"), Some("1"));
        assert_eq!(ctx.get_var("b"), Some("2"));
    }

    #[test]
    fn test_comma_operator_side_effects() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "0").unwrap();
        // All expressions should be evaluated for side effects
        assert_eq!(evaluate_arithmetic("x=1, x=2, x=3", &mut ctx).unwrap(), 3);
        assert_eq!(ctx.get_var("x"), Some("3"));
    }

    #[test]
    fn test_comma_operator_with_increment() {
        let mut ctx = Context::empty();
        ctx.set_var("i", "0").unwrap();
        // Common for-loop style: init, condition check
        assert_eq!(evaluate_arithmetic("i++, i++, i++", &mut ctx).unwrap(), 2);
        assert_eq!(ctx.get_var("i"), Some("3"));
    }

    #[test]
    fn test_comma_operator_in_parens() {
        let mut ctx = Context::empty();
        // Comma operator inside parentheses
        assert_eq!(evaluate_arithmetic("2 * (1, 2, 3)", &mut ctx).unwrap(), 6);
    }
}
