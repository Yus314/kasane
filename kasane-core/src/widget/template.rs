//! Template parsing and expansion.

use compact_str::CompactString;

use super::condition::{CondParseError, parse_condition};
use super::types::{Template, TemplateAlign, TemplateFmt, TemplateSegment};
use super::variables::VariableResolver;

/// Error from parsing a template string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateParseError {
    UnclosedBrace,
    EmptyVariable,
    /// Error parsing inline conditional predicate.
    ConditionalError(CondParseError),
    /// Missing `then` branch after `?condition:`.
    ConditionalMissingThen,
}

impl std::fmt::Display for TemplateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedBrace => write!(f, "unclosed '{{' in template"),
            Self::EmptyVariable => write!(f, "empty variable name in template"),
            Self::ConditionalError(e) => write!(f, "inline conditional: {e}"),
            Self::ConditionalMissingThen => {
                write!(f, "inline conditional: missing ':' after condition")
            }
        }
    }
}

impl Template {
    /// Parse a template string like `" {cursor_line}:{cursor_col} "`.
    ///
    /// Supports inline conditionals: `{?editor_mode == 'insert':INS}` or
    /// `{?condition:then:else}`. Branches can contain nested variables and
    /// conditionals: `{?is_focused:{cursor_line}:N/A}`.
    pub fn parse(input: &str) -> Result<Self, TemplateParseError> {
        let segments = parse_segments(input)?;
        Ok(Template { segments })
    }

    /// Expand this template against a variable resolver.
    pub fn expand(&self, resolver: &dyn VariableResolver) -> CompactString {
        let mut result = String::new();
        expand_segments(&self.segments, resolver, &mut result);
        CompactString::from(result)
    }

    /// Iterate over all variable names referenced in this template.
    pub fn referenced_variables(&self) -> Vec<&str> {
        let mut vars = Vec::new();
        collect_referenced_variables(&self.segments, &mut vars);
        vars
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

/// Scan content inside braces, tracking nested brace depth.
///
/// Assumes the opening `{` has already been consumed. Returns the content
/// between the opening `{` and the matching closing `}`.
fn scan_braced_content(
    chars: &mut impl Iterator<Item = char>,
) -> Result<String, TemplateParseError> {
    let mut content = String::new();
    let mut depth = 0u32;
    for ch in chars {
        match ch {
            '{' => {
                depth += 1;
                content.push(ch);
            }
            '}' => {
                if depth > 0 {
                    depth -= 1;
                    content.push(ch);
                } else {
                    return Ok(content);
                }
            }
            _ => content.push(ch),
        }
    }
    Err(TemplateParseError::UnclosedBrace)
}

/// Find the byte position of the last `:` at brace-depth 0 in the given string.
fn find_last_depth0_colon(s: &str) -> Option<usize> {
    let mut last = None;
    let mut depth = 0u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            ':' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// Parse a string into template segments (supports variables and nested conditionals).
fn parse_segments(input: &str) -> Result<Vec<TemplateSegment>, TemplateParseError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            if !literal.is_empty() {
                segments.push(TemplateSegment::Literal(CompactString::from(&literal)));
                literal.clear();
            }
            let var = scan_braced_content(&mut chars)?;
            if var.is_empty() {
                return Err(TemplateParseError::EmptyVariable);
            }

            if let Some(rest) = var.strip_prefix('?') {
                let seg = parse_conditional(rest)?;
                segments.push(seg);
            } else {
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
            }
        } else {
            literal.push(ch);
        }
    }

    if !literal.is_empty() {
        segments.push(TemplateSegment::Literal(CompactString::from(literal)));
    }

    Ok(segments)
}

/// Parse an inline conditional: the content after `?` inside `{?...}`.
///
/// Format: `condition:then_branch` or `condition:then_branch:else_branch`.
/// Branches can contain nested `{variable}` and `{?conditional}` expressions.
fn parse_conditional(input: &str) -> Result<TemplateSegment, TemplateParseError> {
    // Find the first `:` that separates the condition from the then branch.
    // Condition expressions don't use `:`, so the first one is always the separator.
    let colon_pos = input
        .find(':')
        .ok_or(TemplateParseError::ConditionalMissingThen)?;

    let cond_str = input[..colon_pos].trim();
    let rest = &input[colon_pos + 1..];

    let predicate = parse_condition(cond_str).map_err(TemplateParseError::ConditionalError)?;

    // Split then/else on the last depth-0 `:` in the rest.
    // Depth-aware scan ensures braces in branches (e.g. `{var:fmt}`) don't
    // interfere with the then/else separator.
    let (then_str, else_str) = if let Some(colon2) = find_last_depth0_colon(rest) {
        (&rest[..colon2], &rest[colon2 + 1..])
    } else {
        (rest, "")
    };

    let then_segments = parse_segments(then_str)?;
    let else_segments = if else_str.is_empty() {
        Vec::new()
    } else {
        parse_segments(else_str)?
    };

    Ok(TemplateSegment::Conditional {
        predicate,
        then_segments,
        else_segments,
    })
}

/// Expand a list of segments into a string buffer.
fn expand_segments(
    segments: &[TemplateSegment],
    resolver: &dyn VariableResolver,
    result: &mut String,
) {
    for seg in segments {
        match seg {
            TemplateSegment::Literal(s) => result.push_str(s),
            TemplateSegment::Variable { name, format } => {
                let value = resolver.resolve(name).to_display();
                if let Some(fmt) = format {
                    expand_formatted(&value, fmt, result);
                } else {
                    result.push_str(&value);
                }
            }
            TemplateSegment::Conditional {
                predicate,
                then_segments,
                else_segments,
            } => {
                if predicate.evaluate_with_resolver(resolver) {
                    expand_segments(then_segments, resolver, result);
                } else {
                    expand_segments(else_segments, resolver, result);
                }
            }
        }
    }
}

/// Expand a formatted variable value (truncation + width padding).
fn expand_formatted(value: &str, fmt: &TemplateFmt, result: &mut String) {
    // Apply truncation first (operates on char count)
    let truncated;
    let char_count;
    let display = if let Some(max) = fmt.truncate
        && value.chars().count() > max
    {
        if max > 0 {
            let prefix: String = value.chars().take(max.saturating_sub(1)).collect();
            truncated = format!("{prefix}\u{2026}");
            char_count = max;
            &truncated
        } else {
            char_count = 0;
            ""
        }
    } else {
        char_count = value.chars().count();
        value
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
}

/// Collect all variable names from segments, including those inside conditionals.
fn collect_referenced_variables<'a>(segments: &'a [TemplateSegment], vars: &mut Vec<&'a str>) {
    for seg in segments {
        match seg {
            TemplateSegment::Variable { name, .. } => vars.push(name.as_str()),
            TemplateSegment::Conditional {
                predicate,
                then_segments,
                else_segments,
            } => {
                predicate.collect_variables(vars);
                collect_referenced_variables(then_segments, vars);
                collect_referenced_variables(else_segments, vars);
            }
            TemplateSegment::Literal(_) => {}
        }
    }
}
