//! Recursive descent parser and evaluator for condition expressions.

use compact_str::CompactString;

use super::predicate::Predicate;
use super::types::{CmpOp, Value};

/// Error from parsing a condition expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CondParseError {
    UnexpectedEnd,
    UnexpectedToken(String),
    TooManyNodes,
    /// Condition expression exceeds the maximum length.
    TooLong,
    UnclosedParen,
    /// Invalid regex pattern in `=~` operator.
    InvalidRegex(String),
    /// Missing parenthesized value list after `in`.
    InvalidInSyntax(String),
}

impl std::fmt::Display for CondParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "unexpected end of condition expression"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token in condition: '{t}'"),
            Self::TooManyNodes => write!(f, "condition expression too complex (max 16 nodes)"),
            Self::TooLong => write!(f, "condition expression too long (max 256 characters)"),
            Self::UnclosedParen => write!(f, "unclosed '(' in condition expression"),
            Self::InvalidRegex(e) => write!(f, "invalid regex in '=~': {e}"),
            Self::InvalidInSyntax(e) => write!(f, "invalid 'in' syntax: {e}"),
        }
    }
}

const MAX_NODES: usize = 16;

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    node_count: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            node_count: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<&'a str> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return None;
        }
        let rest = &self.input[self.pos..];

        // Two-char operators
        for op in &["==", "!=", ">=", "<=", "||", "&&", "=~"] {
            if rest.starts_with(op) {
                return Some(&rest[..2]);
            }
        }
        // Single-char operators
        for op in &["!", ">", "<", "(", ")"] {
            if rest.starts_with(op) {
                return Some(&rest[..1]);
            }
        }

        // Quoted string
        if let Some(stripped) = rest.strip_prefix('\'') {
            if let Some(end) = stripped.find('\'') {
                return Some(&rest[..end + 2]);
            }
            return Some(rest); // unterminated, will be handled as error
        }

        // Bare word/number
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || "!=<>&|'()".contains(c))
            .unwrap_or(rest.len());
        if end > 0 { Some(&rest[..end]) } else { None }
    }

    fn advance(&mut self, len: usize) {
        self.pos += len;
    }

    fn consume(&mut self) -> Option<&'a str> {
        let token = self.peek()?;
        self.advance(token.len());
        Some(token)
    }

    fn bump_node(&mut self) -> Result<(), CondParseError> {
        self.node_count += 1;
        if self.node_count > MAX_NODES {
            return Err(CondParseError::TooManyNodes);
        }
        Ok(())
    }

    /// `expr := or_expr`
    fn parse_expr(&mut self) -> Result<Predicate, CondParseError> {
        self.parse_or()
    }

    /// `or_expr := and_expr ("||" and_expr)*`
    fn parse_or(&mut self) -> Result<Predicate, CondParseError> {
        let mut left = self.parse_and()?;
        while self.peek() == Some("||") {
            self.advance(2);
            self.bump_node()?;
            let right = self.parse_and()?;
            left = Predicate::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `and_expr := not_expr ("&&" not_expr)*`
    fn parse_and(&mut self) -> Result<Predicate, CondParseError> {
        let mut left = self.parse_not()?;
        while self.peek() == Some("&&") {
            self.advance(2);
            self.bump_node()?;
            let right = self.parse_not()?;
            left = Predicate::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `not_expr := "!" primary | primary`
    fn parse_not(&mut self) -> Result<Predicate, CondParseError> {
        if self.peek() == Some("!") {
            self.advance(1);
            self.bump_node()?;
            let inner = self.parse_primary()?;
            return Ok(Predicate::Not(Box::new(inner)));
        }
        self.parse_primary()
    }

    /// `primary := "(" expr ")" | atom`
    fn parse_primary(&mut self) -> Result<Predicate, CondParseError> {
        if self.peek() == Some("(") {
            self.advance(1);
            let inner = self.parse_expr()?;
            if self.peek() != Some(")") {
                return Err(CondParseError::UnclosedParen);
            }
            self.advance(1);
            return Ok(inner);
        }
        self.parse_atom()
    }

    /// `atom := variable op value | variable "in" "(" values ")" | variable`
    fn parse_atom(&mut self) -> Result<Predicate, CondParseError> {
        self.bump_node()?;
        let token = self.consume().ok_or(CondParseError::UnexpectedEnd)?;

        // This should be a variable name (bare word)
        if is_operator(token) {
            return Err(CondParseError::UnexpectedToken(token.to_string()));
        }
        let variable = CompactString::from(token);

        // Check if next token is a comparison operator
        let next = self.peek();
        let op = match next {
            Some("==") => Some(CmpOp::Eq),
            Some("!=") => Some(CmpOp::Ne),
            Some(">") => Some(CmpOp::Gt),
            Some("<") => Some(CmpOp::Lt),
            Some(">=") => Some(CmpOp::Ge),
            Some("<=") => Some(CmpOp::Le),
            Some("=~") => Some(CmpOp::Matches),
            _ => None,
        };

        if let Some(op) = op {
            let op_token = self.consume().unwrap();
            let _ = op_token;

            if op == CmpOp::Matches {
                // Regex match: compile RHS as regex at parse time.
                let pattern_token = self.consume().ok_or(CondParseError::UnexpectedEnd)?;
                let pattern_str = unquote(pattern_token);
                let regex = regex_lite::Regex::new(&pattern_str)
                    .map_err(|e| CondParseError::InvalidRegex(e.to_string()))?;
                return Ok(Predicate::VariableMatches {
                    variable,
                    pattern: std::sync::Arc::new(regex),
                });
            }

            let value_token = self.consume().ok_or(CondParseError::UnexpectedEnd)?;
            let value = parse_literal_value(value_token);

            Ok(Predicate::VariableCompare {
                variable,
                op,
                value,
            })
        } else if next == Some("in") {
            // Set membership: variable in ('a', 'b', 'c')
            self.consume(); // consume "in"
            self.parse_in_values(variable)
        } else {
            Ok(Predicate::VariableTruthy(variable))
        }
    }

    /// Parse the `('value1', 'value2', ...)` part of a `variable in (...)` expression.
    fn parse_in_values(&mut self, variable: CompactString) -> Result<Predicate, CondParseError> {
        self.skip_whitespace();
        if self.pos >= self.input.len() || self.input.as_bytes()[self.pos] != b'(' {
            return Err(CondParseError::InvalidInSyntax(
                "expected '(' after 'in'".to_string(),
            ));
        }
        self.pos += 1; // consume '('

        // Find matching ')' and extract the content between parens.
        let start = self.pos;
        let mut depth = 1u32;
        while self.pos < self.input.len() {
            match self.input.as_bytes()[self.pos] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        if depth != 0 {
            return Err(CondParseError::InvalidInSyntax(
                "unclosed '(' in 'in' value list".to_string(),
            ));
        }
        let content = &self.input[start..self.pos];
        self.pos += 1; // consume ')'

        // Split by commas and parse each value.
        let mut values = Vec::new();
        for item in content.split(',') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                values.push(parse_literal_value(trimmed));
            }
        }

        Ok(Predicate::VariableIn { variable, values })
    }
}

fn is_operator(s: &str) -> bool {
    matches!(
        s,
        "==" | "!=" | ">" | "<" | ">=" | "<=" | "||" | "&&" | "!" | "(" | ")" | "=~"
    )
}

/// Strip surrounding single quotes from a token, if present.
fn unquote(token: &str) -> String {
    if token.starts_with('\'') && token.ends_with('\'') && token.len() >= 2 {
        token[1..token.len() - 1].to_string()
    } else {
        token.to_string()
    }
}

/// Parse a literal token into a typed `Value`.
///
/// - Quoted strings (`'insert'`) → `Value::Str`
/// - Integer literals (`1`, `-42`) → `Value::Int`
/// - Bare words (`insert`) → `Value::Str`
fn parse_literal_value(token: &str) -> Value {
    if token.starts_with('\'') && token.ends_with('\'') && token.len() >= 2 {
        return Value::Str(CompactString::from(&token[1..token.len() - 1]));
    }
    if let Ok(n) = token.parse::<i64>() {
        return Value::Int(n);
    }
    Value::Str(CompactString::from(token))
}

/// Parse a condition expression string.
pub fn parse_condition(expr: &str) -> Result<Predicate, CondParseError> {
    if expr.len() > 256 {
        return Err(CondParseError::TooLong);
    }
    let mut parser = Parser::new(expr.trim());
    let result = parser.parse_expr()?;

    // Ensure all input was consumed
    parser.skip_whitespace();
    if parser.pos < parser.input.len() {
        return Err(CondParseError::UnexpectedToken(
            parser.input[parser.pos..].to_string(),
        ));
    }
    Ok(result)
}

// evaluate() and referenced_variables() are now on Predicate in predicate.rs
