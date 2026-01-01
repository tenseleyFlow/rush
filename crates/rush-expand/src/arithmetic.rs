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
/// Supports: +, -, *, /, %, (), variables
pub fn evaluate_arithmetic(expr: &str, context: &Context) -> Result<i64, ArithmeticError> {
    let tokens = tokenize(expr)?;
    let mut parser = Parser::new(tokens, context);
    parser.parse_expression()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Variable(String),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    LeftParen,
    RightParen,
}

/// Tokenize an arithmetic expression
fn tokenize(expr: &str) -> Result<Vec<Token>, ArithmeticError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                chars.next();
                // Check if it's a negative number
                if tokens.is_empty() || matches!(tokens.last(), Some(Token::LeftParen) | Some(Token::Plus) | Some(Token::Minus) | Some(Token::Multiply) | Some(Token::Divide) | Some(Token::Modulo)) {
                    // It's a negative sign for a number
                    let num = parse_number(&mut chars)?;
                    tokens.push(Token::Number(-num));
                } else {
                    tokens.push(Token::Minus);
                }
            }
            '*' => {
                tokens.push(Token::Multiply);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Divide);
                chars.next();
            }
            '%' => {
                tokens.push(Token::Modulo);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LeftParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RightParen);
                chars.next();
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
    context: &'a Context,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token>, context: &'a Context) -> Self {
        Self { tokens, pos: 0, context }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    /// Parse expression: term (('+' | '-') term)*
    fn parse_expression(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_term()?;

        while let Some(token) = self.current() {
            match token {
                Token::Plus => {
                    self.advance();
                    result += self.parse_term()?;
                }
                Token::Minus => {
                    self.advance();
                    result -= self.parse_term()?;
                }
                _ => break,
            }
        }

        Ok(result)
    }

    /// Parse term: factor (('*' | '/' | '%') factor)*
    fn parse_term(&mut self) -> Result<i64, ArithmeticError> {
        let mut result = self.parse_factor()?;

        while let Some(token) = self.current() {
            match token {
                Token::Multiply => {
                    self.advance();
                    result *= self.parse_factor()?;
                }
                Token::Divide => {
                    self.advance();
                    let divisor = self.parse_factor()?;
                    if divisor == 0 {
                        return Err(ArithmeticError::DivisionByZero);
                    }
                    result /= divisor;
                }
                Token::Modulo => {
                    self.advance();
                    let divisor = self.parse_factor()?;
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

    /// Parse factor: number | variable | '(' expression ')'
    fn parse_factor(&mut self) -> Result<i64, ArithmeticError> {
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
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("2 + 3", &ctx).unwrap(), 5);
    }

    #[test]
    fn test_multiplication() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("4 * 5", &ctx).unwrap(), 20);
    }

    #[test]
    fn test_complex_expression() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("(2 + 3) * 4", &ctx).unwrap(), 20);
    }

    #[test]
    fn test_with_variable() {
        let mut ctx = Context::empty();
        ctx.set_var("x", "10").unwrap();
        assert_eq!(evaluate_arithmetic("x * 2", &ctx).unwrap(), 20);
    }

    #[test]
    fn test_division() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("10 / 2", &ctx).unwrap(), 5);
    }

    #[test]
    fn test_modulo() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("10 % 3", &ctx).unwrap(), 1);
    }

    #[test]
    fn test_negative_numbers() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("-5 + 3", &ctx).unwrap(), -2);
    }

    #[test]
    fn test_precedence() {
        let ctx = Context::empty();
        assert_eq!(evaluate_arithmetic("2 + 3 * 4", &ctx).unwrap(), 14);
    }
}
