//! Type conversions between WIT-generated types and kasane-core types.

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::protocol::{Attributes, Color, Face, NamedColor};

pub(crate) fn wit_face_to_face(wf: &wit::Face) -> Face {
    Face {
        fg: wit_color_to_color(&wf.fg),
        bg: wit_color_to_color(&wf.bg),
        underline: wit_color_to_color(&wf.underline),
        attributes: Attributes::from_bits_truncate(wf.attributes),
    }
}

fn wit_color_to_color(wc: &wit::Color) -> Color {
    match wc {
        wit::Color::DefaultColor => Color::Default,
        wit::Color::Named(n) => Color::Named(wit_named_to_named(*n)),
        wit::Color::Rgb(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}

fn wit_named_to_named(wn: wit::NamedColor) -> NamedColor {
    match wn {
        wit::NamedColor::Black => NamedColor::Black,
        wit::NamedColor::Red => NamedColor::Red,
        wit::NamedColor::Green => NamedColor::Green,
        wit::NamedColor::Yellow => NamedColor::Yellow,
        wit::NamedColor::Blue => NamedColor::Blue,
        wit::NamedColor::Magenta => NamedColor::Magenta,
        wit::NamedColor::Cyan => NamedColor::Cyan,
        wit::NamedColor::White => NamedColor::White,
        wit::NamedColor::BrightBlack => NamedColor::BrightBlack,
        wit::NamedColor::BrightRed => NamedColor::BrightRed,
        wit::NamedColor::BrightGreen => NamedColor::BrightGreen,
        wit::NamedColor::BrightYellow => NamedColor::BrightYellow,
        wit::NamedColor::BrightBlue => NamedColor::BrightBlue,
        wit::NamedColor::BrightMagenta => NamedColor::BrightMagenta,
        wit::NamedColor::BrightCyan => NamedColor::BrightCyan,
        wit::NamedColor::BrightWhite => NamedColor::BrightWhite,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_default_color() {
        let wc = wit::Color::DefaultColor;
        assert_eq!(wit_color_to_color(&wc), Color::Default);
    }

    #[test]
    fn convert_rgb_color() {
        let wc = wit::Color::Rgb(wit::RgbColor {
            r: 40,
            g: 40,
            b: 50,
        });
        assert_eq!(
            wit_color_to_color(&wc),
            Color::Rgb {
                r: 40,
                g: 40,
                b: 50
            }
        );
    }

    #[test]
    fn convert_named_color() {
        let wc = wit::Color::Named(wit::NamedColor::BrightCyan);
        assert_eq!(
            wit_color_to_color(&wc),
            Color::Named(NamedColor::BrightCyan)
        );
    }

    #[test]
    fn convert_face_with_attributes() {
        let wf = wit::Face {
            fg: wit::Color::Named(wit::NamedColor::Red),
            bg: wit::Color::Rgb(wit::RgbColor {
                r: 10,
                g: 20,
                b: 30,
            }),
            underline: wit::Color::DefaultColor,
            attributes: 0x20, // BOLD
        };
        let f = wit_face_to_face(&wf);
        assert_eq!(f.fg, Color::Named(NamedColor::Red));
        assert_eq!(
            f.bg,
            Color::Rgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert_eq!(f.underline, Color::Default);
        assert!(f.attributes.contains(Attributes::BOLD));
    }
}
