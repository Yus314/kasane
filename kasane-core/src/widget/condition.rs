//! Recursive descent parser and evaluator for condition expressions.

use compact_str::CompactString;

use super::types::{CmpOp, CondExpr};
use super::variables::VariableResolver;

/// Error from parsing a condition expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CondParseError {
    UnexpectedEnd,
    UnexpectedToken(String),
    TooManyNodes,
}

impl std::fmt::Display for CondParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "unexpected end of condition expression"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token in condition: '{t}'"),
            Self::TooManyNodes => write!(f, "condition expression too complex (max 16 nodes)"),
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
        for op in &["==", "!=", ">=", "<=", "||", "&&"] {
            if rest.starts_with(op) {
                return Some(&rest[..2]);
            }
        }
        // Single-char operators
        for op in &["!", ">", "<"] {
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
            .find(|c: char| c.is_ascii_whitespace() || "!=<>&|'".contains(c))
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
    fn parse_expr(&mut self) -> Result<CondExpr, CondParseError> {
        self.parse_or()
    }

    /// `or_expr := and_expr ("||" and_expr)*`
    fn parse_or(&mut self) -> Result<CondExpr, CondParseError> {
        let mut left = self.parse_and()?;
        while self.peek() == Some("||") {
            self.advance(2);
            self.bump_node()?;
            let right = self.parse_and()?;
            left = CondExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `and_expr := not_expr ("&&" not_expr)*`
    fn parse_and(&mut self) -> Result<CondExpr, CondParseError> {
        let mut left = self.parse_not()?;
        while self.peek() == Some("&&") {
            self.advance(2);
            self.bump_node()?;
            let right = self.parse_not()?;
            left = CondExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `not_expr := "!" atom | atom`
    fn parse_not(&mut self) -> Result<CondExpr, CondParseError> {
        if self.peek() == Some("!") {
            self.advance(1);
            self.bump_node()?;
            let inner = self.parse_atom()?;
            return Ok(CondExpr::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    /// `atom := variable op value | variable`
    fn parse_atom(&mut self) -> Result<CondExpr, CondParseError> {
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
            _ => None,
        };

        if let Some(op) = op {
            let op_token = self.consume().unwrap();
            self.advance(0); // just to consume whitespace via peek
            let _ = op_token;

            let value_token = self.consume().ok_or(CondParseError::UnexpectedEnd)?;
            let value = if value_token.starts_with('\'') && value_token.ends_with('\'') {
                CompactString::from(&value_token[1..value_token.len() - 1])
            } else {
                CompactString::from(value_token)
            };

            Ok(CondExpr::Compare {
                variable,
                op,
                value,
            })
        } else {
            Ok(CondExpr::Truthy(variable))
        }
    }
}

fn is_operator(s: &str) -> bool {
    matches!(s, "==" | "!=" | ">" | "<" | ">=" | "<=" | "||" | "&&" | "!")
}

/// Parse a condition expression string.
pub fn parse_condition(expr: &str) -> Result<CondExpr, CondParseError> {
    if expr.len() > 256 {
        return Err(CondParseError::TooManyNodes);
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

impl CondExpr {
    /// Evaluate this condition against a variable resolver.
    pub fn evaluate(&self, resolver: &dyn VariableResolver) -> bool {
        match self {
            Self::Truthy(name) => {
                let val = resolver.resolve(name);
                is_truthy(&val)
            }
            Self::Compare {
                variable,
                op,
                value,
            } => {
                let resolved = resolver.resolve(variable);
                compare(&resolved, *op, value)
            }
            Self::And(a, b) => a.evaluate(resolver) && b.evaluate(resolver),
            Self::Or(a, b) => a.evaluate(resolver) || b.evaluate(resolver),
            Self::Not(inner) => !inner.evaluate(resolver),
        }
    }

    /// Iterate over all variable names referenced in this expression.
    pub fn referenced_variables(&self) -> Vec<&str> {
        let mut vars = Vec::new();
        collect_variables(self, &mut vars);
        vars
    }
}

fn collect_variables<'a>(expr: &'a CondExpr, out: &mut Vec<&'a str>) {
    match expr {
        CondExpr::Truthy(name) => out.push(name),
        CondExpr::Compare { variable, .. } => out.push(variable),
        CondExpr::And(a, b) | CondExpr::Or(a, b) => {
            collect_variables(a, out);
            collect_variables(b, out);
        }
        CondExpr::Not(inner) => collect_variables(inner, out),
    }
}

/// Truthiness: empty string and "0" are falsy; everything else is truthy.
fn is_truthy(val: &str) -> bool {
    !val.is_empty() && val != "0"
}

/// Compare two values. Attempt numeric parse first; otherwise lexicographic.
fn compare(left: &str, op: CmpOp, right: &str) -> bool {
    // Try numeric comparison
    if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
        return match op {
            CmpOp::Eq => (l - r).abs() < f64::EPSILON,
            CmpOp::Ne => (l - r).abs() >= f64::EPSILON,
            CmpOp::Gt => l > r,
            CmpOp::Lt => l < r,
            CmpOp::Ge => l >= r,
            CmpOp::Le => l <= r,
        };
    }
    // Lexicographic fallback
    match op {
        CmpOp::Eq => left == right,
        CmpOp::Ne => left != right,
        CmpOp::Gt => left > right,
        CmpOp::Lt => left < right,
        CmpOp::Ge => left >= right,
        CmpOp::Le => left <= right,
    }
}
