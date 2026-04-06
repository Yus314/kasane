//! Template parsing and expansion.

use compact_str::CompactString;

use super::types::{Template, TemplateFmt, TemplateSegment};
use super::variables::VariableResolver;

/// Error from parsing a template string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateParseError {
    UnclosedBrace,
    EmptyVariable,
}

impl std::fmt::Display for TemplateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedBrace => write!(f, "unclosed '{{' in template"),
            Self::EmptyVariable => write!(f, "empty variable name in template"),
        }
    }
}

impl Template {
    /// Parse a template string like `" {cursor_line}:{cursor_col} "`.
    pub fn parse(input: &str) -> Result<Self, TemplateParseError> {
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Collect the variable name and optional format
                if !literal.is_empty() {
                    segments.push(TemplateSegment::Literal(CompactString::from(&literal)));
                    literal.clear();
                }
                let mut var = String::new();
                let mut found_close = false;
                for ch in chars.by_ref() {
                    if ch == '}' {
                        found_close = true;
                        break;
                    }
                    var.push(ch);
                }
                if !found_close {
                    return Err(TemplateParseError::UnclosedBrace);
                }
                if var.is_empty() {
                    return Err(TemplateParseError::EmptyVariable);
                }

                // Check for format spec: {name:width}
                let (name, format) = if let Some(colon_pos) = var.find(':') {
                    let name_part = &var[..colon_pos];
                    let fmt_part = &var[colon_pos + 1..];
                    let width = fmt_part.parse::<usize>().unwrap_or(0);
                    (
                        name_part.to_string(),
                        if width > 0 {
                            Some(TemplateFmt { width })
                        } else {
                            None
                        },
                    )
                } else {
                    (var, None)
                };

                segments.push(TemplateSegment::Variable {
                    name: CompactString::from(name),
                    format,
                });
            } else {
                literal.push(ch);
            }
        }

        if !literal.is_empty() {
            segments.push(TemplateSegment::Literal(CompactString::from(literal)));
        }

        Ok(Template { segments })
    }

    /// Expand this template against a variable resolver.
    pub fn expand(&self, resolver: &dyn VariableResolver) -> CompactString {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                TemplateSegment::Literal(s) => result.push_str(s),
                TemplateSegment::Variable { name, format } => {
                    let value = resolver.resolve(name);
                    if let Some(fmt) = format {
                        // Right-align to width
                        if value.len() < fmt.width {
                            for _ in 0..fmt.width - value.len() {
                                result.push(' ');
                            }
                        }
                        result.push_str(&value);
                    } else {
                        result.push_str(&value);
                    }
                }
            }
        }
        CompactString::from(result)
    }

    /// Iterate over all variable names referenced in this template.
    pub fn referenced_variables(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().filter_map(|seg| match seg {
            TemplateSegment::Variable { name, .. } => Some(name.as_str()),
            TemplateSegment::Literal(_) => None,
        })
    }
}
