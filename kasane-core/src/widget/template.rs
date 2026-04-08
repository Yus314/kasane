//! Template parsing and expansion.

use compact_str::CompactString;

use super::types::{Template, TemplateAlign, TemplateFmt, TemplateSegment};
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

                // Check for format spec: {name:[<]width[.truncate]}
                let (name, format) = if let Some(colon_pos) = var.find(':') {
                    let name_part = &var[..colon_pos];
                    let fmt_part = &var[colon_pos + 1..];
                    let fmt = parse_format_spec(fmt_part);
                    (name_part.to_string(), fmt)
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
                        // Apply truncation first (operates on char count)
                        let truncated;
                        let char_count;
                        let display = if let Some(max) = fmt.truncate
                            && value.chars().count() > max
                        {
                            if max > 0 {
                                let prefix: String =
                                    value.chars().take(max.saturating_sub(1)).collect();
                                truncated = format!("{prefix}\u{2026}");
                                char_count = max;
                                &truncated
                            } else {
                                char_count = 0;
                                ""
                            }
                        } else {
                            char_count = value.chars().count();
                            value.as_str()
                        };

                        // Apply width padding
                        if let Some(width) = fmt.width
                            && char_count < width
                        {
                            let padding = width - char_count;
                            match fmt.align {
                                TemplateAlign::Right => {
                                    for _ in 0..padding {
                                        result.push(' ');
                                    }
                                    result.push_str(display);
                                }
                                TemplateAlign::Left => {
                                    result.push_str(display);
                                    for _ in 0..padding {
                                        result.push(' ');
                                    }
                                }
                            }
                        } else {
                            result.push_str(display);
                        }
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

/// Parse a format spec string like `"<10"`, `".20"`, `"<10.20"`, `"10"`.
///
/// Grammar: `[<]?[\d]*[.\d+]?`
fn parse_format_spec(spec: &str) -> Option<TemplateFmt> {
    let mut s = spec;
    let mut align = TemplateAlign::Right;

    // Check for `<` prefix (left-align)
    if let Some(rest) = s.strip_prefix('<') {
        align = TemplateAlign::Left;
        s = rest;
    }

    // Split on `.` for truncation
    let (width_part, truncate) = if let Some(dot_pos) = s.find('.') {
        let w = &s[..dot_pos];
        let t = s[dot_pos + 1..].parse::<usize>().ok();
        (w, t)
    } else {
        (s, None)
    };

    let width = if width_part.is_empty() {
        None
    } else {
        width_part.parse::<usize>().ok().filter(|&w| w > 0)
    };

    if width.is_none() && truncate.is_none() && align == TemplateAlign::Right {
        return None;
    }

    Some(TemplateFmt {
        width,
        align,
        truncate,
    })
}
