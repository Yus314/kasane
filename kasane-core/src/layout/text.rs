use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Line};

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

/// The clippy assistant from Kakoune's terminal UI.
/// Each line is exactly 8 display columns wide.
pub(crate) const ASSISTANT_CLIPPY: &[&str] = &[
    " ╭──╮  ",
    " │  │  ",
    " @  @  ╭",
    " ││ ││ │",
    " ││ ││ ╯",
    " │╰─╯│ ",
    " ╰───╯ ",
    "        ",
];
/// Display width of each assistant line.
pub(crate) const ASSISTANT_WIDTH: u16 = 8;

pub fn line_display_width(line: &[Atom]) -> usize {
    line.iter()
        .map(|atom| {
            atom.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
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
    use crate::test_utils::make_line;

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

    #[test]
    fn test_line_display_width_excludes_control_chars() {
        // Control characters like \n and \r should not contribute to display width
        let line = make_line("hello\nworld");
        assert_eq!(line_display_width(&line), 10);

        let line = make_line("abc\r\ndef");
        assert_eq!(line_display_width(&line), 6);

        let line = make_line("\x01\x02\x03");
        assert_eq!(line_display_width(&line), 0);
    }
}
