// ---------------------------------------------------------------------------
// Image reference detection
// ---------------------------------------------------------------------------

/// Supported image file extensions.
const IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".svg"];

fn is_image_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Try to extract an image path from a Markdown image reference: `![alt](path)`
fn detect_markdown_image(text: &str, cursor_byte: usize) -> Option<String> {
    // Find `![` before or at cursor
    let bytes = text.as_bytes();
    let mut search_start = cursor_byte.min(text.len());
    // Walk back to find `![`
    loop {
        if search_start >= 2 && bytes[search_start - 2] == b'!' && bytes[search_start - 1] == b'['
        {
            // Found potential start at search_start - 2
            break;
        }
        if search_start == 0 {
            // Also try forward scan from line start
            break;
        }
        search_start -= 1;
    }

    // Scan all `![...](...)`  on the line and check if cursor is inside
    let mut i = 0;
    while i + 4 < bytes.len() {
        if bytes[i] == b'!' && bytes[i + 1] == b'[' {
            let start = i;
            // Find closing `]`
            let Some(close_bracket) = text[i + 2..].find(']') else {
                i += 2;
                continue;
            };
            let after_bracket = i + 2 + close_bracket + 1;
            // Expect `(`
            if after_bracket >= bytes.len() || bytes[after_bracket] != b'(' {
                i += 2;
                continue;
            }
            let path_start = after_bracket + 1;
            let Some(close_paren) = text[path_start..].find(')') else {
                i += 2;
                continue;
            };
            let path_end = path_start + close_paren;
            let end = path_end + 1; // past the ')'

            if cursor_byte >= start && cursor_byte < end {
                let path = text[path_start..path_end].trim();
                if !path.is_empty() && is_image_extension(path) {
                    return Some(path.to_string());
                }
            }

            i = end;
        } else {
            i += 1;
        }
    }
    None
}

/// Try to extract an image path from an HTML img tag: `<img src="path">`
fn detect_html_image(text: &str, cursor_byte: usize) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut search_from = 0;

    while let Some(tag_start) = lower[search_from..].find("<img ") {
        let abs_start = search_from + tag_start;
        let tag_end = match text[abs_start..].find('>') {
            Some(pos) => abs_start + pos + 1,
            None => text.len(),
        };

        if cursor_byte >= abs_start && cursor_byte < tag_end {
            // Extract src="..."
            let tag_content = &text[abs_start..tag_end];
            let tag_lower = tag_content.to_ascii_lowercase();
            if let Some(src_pos) = tag_lower.find("src=") {
                let after_src = &tag_content[src_pos + 4..];
                let (path, _) = if after_src.starts_with('"') {
                    after_src[1..].split_once('"')?
                } else if after_src.starts_with('\'') {
                    after_src[1..].split_once('\'')?
                } else {
                    let end = after_src
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after_src.len());
                    (&after_src[..end], "")
                };
                let path = path.trim();
                if !path.is_empty() && is_image_extension(path) {
                    return Some(path.to_string());
                }
            }
        }

        search_from = abs_start + 5;
    }
    None
}

/// Try to detect a bare image file path token at the cursor position.
fn detect_bare_path(text: &str, cursor_byte: usize) -> Option<String> {
    if cursor_byte > text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    // Expand around cursor to find a whitespace-delimited token
    let mut start = cursor_byte;
    while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    let mut end = cursor_byte;
    while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    if start == end {
        return None;
    }
    let token = text[start..end].trim_matches(|c: char| "\"'`()[]{}".contains(c));
    if is_image_extension(token) {
        Some(token.to_string())
    } else {
        None
    }
}

/// Detect an image reference at the cursor position.
fn detect_image_ref(text: &str, cursor_col: usize) -> Option<String> {
    // Convert column (character offset) to byte offset
    let cursor_byte = text
        .char_indices()
        .nth(cursor_col)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    detect_markdown_image(text, cursor_byte)
        .or_else(|| detect_html_image(text, cursor_byte))
        .or_else(|| detect_bare_path(text, cursor_byte))
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        image_path: Option<String> = None,
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        cursor_line: i32 = -1,
        #[bind(host_state::get_cursor_col(), on: dirty::BUFFER)]
        cursor_col: i32 = -1,
    },

    on_state_changed_effects(dirty) {
        if dirty & dirty::BUFFER != 0 {
            let new_path = host_state::get_line_text(state.cursor_line as u32).and_then(|text| {
                detect_image_ref(&text, state.cursor_col.max(0) as usize)
            });
            state.image_path = new_path;
        }
        effects(vec![])
    },

    overlay(ctx) {
        let path = state.image_path.as_ref()?;

        // Max size: 30 columns × 15 rows
        let max_w: u16 = 30.min(ctx.screen_cols.saturating_sub(4));
        let max_h: u16 = 15.min(ctx.screen_rows.saturating_sub(4));

        let image = image_file(path, max_w, max_h);

        // Title: file name only
        let title = std::path::Path::new(path.as_str())
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        let title_el = text(
            &format!(" {title} "),
            style_with(named(NamedColor::Black), named(NamedColor::Cyan)),
        );

        let inner = column(&[title_el, image]);
        let el = container(inner).border(BorderLineStyle::Rounded).build();

        let mut avoid = ctx.existing_overlays;
        if let Some(menu_rect) = ctx.menu_rect {
            avoid.push(menu_rect);
        }

        Some(OverlayContribution {
            element: el,
            anchor: OverlayAnchor::AnchorPoint(AnchorPointConfig {
                coord: Coord {
                    line: state.cursor_line,
                    column: state.cursor_col,
                },
                prefer_above: true,
                avoid,
            }),
            z_index: 50,
        })
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_markdown_simple() {
        let text = "See ![logo](images/logo.png) for details";
        // cursor on the `!`
        assert_eq!(
            detect_image_ref(text, 4),
            Some("images/logo.png".to_string())
        );
        // cursor inside the path
        assert_eq!(
            detect_image_ref(text, 18),
            Some("images/logo.png".to_string())
        );
    }

    #[test]
    fn detect_markdown_no_image_ext() {
        let text = "See ![link](page.html) here";
        assert_eq!(detect_image_ref(text, 5), None);
    }

    #[test]
    fn detect_markdown_cursor_outside() {
        let text = "before ![alt](pic.png) after";
        // cursor on "before"
        assert_eq!(detect_image_ref(text, 2), None);
        // cursor on "after"
        assert_eq!(detect_image_ref(text, 25), None);
    }

    #[test]
    fn detect_html_img() {
        let text = r#"<img src="photo.jpg" alt="photo">"#;
        assert_eq!(
            detect_image_ref(text, 10),
            Some("photo.jpg".to_string())
        );
    }

    #[test]
    fn detect_html_img_single_quotes() {
        let text = "<img src='photo.webp' />";
        assert_eq!(
            detect_image_ref(text, 5),
            Some("photo.webp".to_string())
        );
    }

    #[test]
    fn detect_bare_path() {
        let text = "open /tmp/screenshot.png now";
        assert_eq!(
            detect_image_ref(text, 10),
            Some("/tmp/screenshot.png".to_string())
        );
    }

    #[test]
    fn detect_bare_path_no_ext() {
        let text = "open /tmp/file.txt now";
        assert_eq!(detect_image_ref(text, 10), None);
    }

    #[test]
    fn detect_markdown_with_spaces_in_alt() {
        let text = "![my cool image](path/to/image.jpeg)";
        assert_eq!(
            detect_image_ref(text, 0),
            Some("path/to/image.jpeg".to_string())
        );
    }

    #[test]
    fn no_detection_on_empty_line() {
        assert_eq!(detect_image_ref("", 0), None);
    }
}
