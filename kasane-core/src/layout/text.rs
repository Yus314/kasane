use unicode_width::UnicodeWidthStr;

use crate::protocol::Line;

/// Kakoune-compatible word character test (`is_word` in `unicode.hh`).
///
/// A character is a "word" character if it is alphanumeric (ASCII or Unicode)
/// or underscore. Non-word characters serve as word boundaries for line wrapping.
pub fn is_word_char(grapheme: &str) -> bool {
    // Match Kakoune: alphanumeric or underscore (extra_word_chars default)
    grapheme.chars().next().is_some_and(|c| {
        if c.is_ascii() {
            c.is_ascii_alphanumeric() || c == '_'
        } else {
            c.is_alphanumeric()
        }
    })
}

/// Width of the Kakoune clippy assistant drawn beside prompt info.
pub const PROMPT_ASSISTANT_WIDTH: u16 = 8;
/// Minimum height for prompt info (to fit the assistant).
pub const PROMPT_ASSISTANT_MIN_HEIGHT: u16 = 7;

pub fn line_display_width(line: &Line) -> usize {
    line.iter()
        .map(|atom| UnicodeWidthStr::width(atom.contents.as_str()))
        .sum()
}

/// Return the index (exclusive) of the last non-empty line in `content`,
/// effectively trimming trailing empty lines.
pub(super) fn trim_trailing_empty(content: &[Line]) -> usize {
    content
        .iter()
        .rposition(|line| line_display_width(line) > 0)
        .map(|i| i + 1)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    fn make_line(s: &str) -> Line {
        vec![Atom {
            face: Face::default(),
            contents: s.to_string(),
        }]
    }

    #[test]
    fn test_trim_trailing_empty_lines() {
        let content = vec![make_line("hello"), make_line(""), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 1);
    }

    #[test]
    fn test_trim_no_trailing_empty() {
        let content = vec![make_line("hello"), make_line("world")];
        assert_eq!(trim_trailing_empty(&content), 2);
    }

    #[test]
    fn test_trim_all_empty() {
        let content = vec![make_line(""), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 0);
    }

    #[test]
    fn test_trim_middle_empty_preserved() {
        let content = vec![make_line("a"), make_line(""), make_line("b"), make_line("")];
        assert_eq!(trim_trailing_empty(&content), 3);
    }
}
