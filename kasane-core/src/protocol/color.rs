use std::fmt;

use bitflags::bitflags;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Named(NamedColor),
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(r##""default", a named color, or "#rrggbb""##)
            }

            fn visit_str<E>(self, v: &str) -> Result<Color, E>
            where
                E: de::Error,
            {
                parse_color(v).ok_or_else(|| de::Error::custom(format!("unknown color: {v}")))
            }
        }

        deserializer.deserialize_str(ColorVisitor)
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Color::Default => serializer.serialize_str("default"),
            Color::Named(n) => serializer.serialize_str(named_color_str(*n)),
            Color::Rgb { r, g, b } => {
                serializer.serialize_str(&format!("rgb:{r:02x}{g:02x}{b:02x}"))
            }
        }
    }
}

fn named_color_str(c: NamedColor) -> &'static str {
    match c {
        NamedColor::Black => "black",
        NamedColor::Red => "red",
        NamedColor::Green => "green",
        NamedColor::Yellow => "yellow",
        NamedColor::Blue => "blue",
        NamedColor::Magenta => "magenta",
        NamedColor::Cyan => "cyan",
        NamedColor::White => "white",
        NamedColor::BrightBlack => "bright-black",
        NamedColor::BrightRed => "bright-red",
        NamedColor::BrightGreen => "bright-green",
        NamedColor::BrightYellow => "bright-yellow",
        NamedColor::BrightBlue => "bright-blue",
        NamedColor::BrightMagenta => "bright-magenta",
        NamedColor::BrightCyan => "bright-cyan",
        NamedColor::BrightWhite => "bright-white",
    }
}

impl NamedColor {
    /// Standard ANSI color to RGB mapping.
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            NamedColor::Black => (0, 0, 0),
            NamedColor::Red => (205, 0, 0),
            NamedColor::Green => (0, 205, 0),
            NamedColor::Yellow => (205, 205, 0),
            NamedColor::Blue => (0, 0, 238),
            NamedColor::Magenta => (205, 0, 205),
            NamedColor::Cyan => (0, 205, 205),
            NamedColor::White => (229, 229, 229),
            NamedColor::BrightBlack => (127, 127, 127),
            NamedColor::BrightRed => (255, 0, 0),
            NamedColor::BrightGreen => (0, 255, 0),
            NamedColor::BrightYellow => (255, 255, 0),
            NamedColor::BrightBlue => (92, 92, 255),
            NamedColor::BrightMagenta => (255, 0, 255),
            NamedColor::BrightCyan => (0, 255, 255),
            NamedColor::BrightWhite => (255, 255, 255),
        }
    }
}

impl Color {
    /// Convert to RGB tuple. Named colors use standard ANSI values.
    /// Returns None for Color::Default (caller must resolve).
    pub fn to_rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Color::Default => None,
            Color::Named(n) => Some(n.to_rgb()),
            Color::Rgb { r, g, b } => Some((r, g, b)),
        }
    }
}

fn parse_color(s: &str) -> Option<Color> {
    if s == "default" {
        return Some(Color::Default);
    }
    // Kakoune sends "rgba:RRGGBBAA" for colors with alpha (e.g. selection faces
    // in third-party themes).  Terminal emulators don't support alpha, so we
    // strip the alpha channel and treat the color as opaque RGB.
    if let Some(hex) = s.strip_prefix("rgba:") {
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            // alpha (&hex[6..8]) intentionally ignored
            return Some(Color::Rgb { r, g, b });
        }
        return None;
    }
    // Kakoune sends "rgb:RRGGBB", also accept "#RRGGBB" for compatibility
    let hex = s.strip_prefix("rgb:").or_else(|| s.strip_prefix('#'));
    if let Some(hex) = hex {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb { r, g, b });
        }
        return None;
    }
    let named = match s {
        "black" => NamedColor::Black,
        "red" => NamedColor::Red,
        "green" => NamedColor::Green,
        "yellow" => NamedColor::Yellow,
        "blue" => NamedColor::Blue,
        "magenta" => NamedColor::Magenta,
        "cyan" => NamedColor::Cyan,
        "white" => NamedColor::White,
        "bright-black" => NamedColor::BrightBlack,
        "bright-red" => NamedColor::BrightRed,
        "bright-green" => NamedColor::BrightGreen,
        "bright-yellow" => NamedColor::BrightYellow,
        "bright-blue" => NamedColor::BrightBlue,
        "bright-magenta" => NamedColor::BrightMagenta,
        "bright-cyan" => NamedColor::BrightCyan,
        "bright-white" => NamedColor::BrightWhite,
        _ => return None,
    };
    Some(Color::Named(named))
}

// ---------------------------------------------------------------------------
// Attributes (bitflags)
// ---------------------------------------------------------------------------

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Attributes: u16 {
        const UNDERLINE        = 1 << 0;
        const CURLY_UNDERLINE  = 1 << 1;
        const DOUBLE_UNDERLINE = 1 << 2;
        const REVERSE          = 1 << 3;
        const BLINK            = 1 << 4;
        const BOLD             = 1 << 5;
        const DIM              = 1 << 6;
        const ITALIC           = 1 << 7;
        const STRIKETHROUGH    = 1 << 8;
        const FINAL_FG         = 1 << 9;
        const FINAL_BG         = 1 << 10;
        const FINAL_ATTR       = 1 << 11;
        const DOTTED_UNDERLINE = 1 << 12;
        const DASHED_UNDERLINE = 1 << 13;
    }
}

fn parse_attribute(s: &str) -> Option<Attributes> {
    Some(match s {
        "underline" => Attributes::UNDERLINE,
        "curly_underline" => Attributes::CURLY_UNDERLINE,
        "double_underline" => Attributes::DOUBLE_UNDERLINE,
        "reverse" => Attributes::REVERSE,
        "blink" => Attributes::BLINK,
        "bold" => Attributes::BOLD,
        "dim" => Attributes::DIM,
        "italic" => Attributes::ITALIC,
        "strikethrough" => Attributes::STRIKETHROUGH,
        "dotted_underline" => Attributes::DOTTED_UNDERLINE,
        "dashed_underline" => Attributes::DASHED_UNDERLINE,
        "final_fg" => Attributes::FINAL_FG,
        "final_bg" => Attributes::FINAL_BG,
        "final_attr" => Attributes::FINAL_ATTR,
        _ => return None,
    })
}

fn attribute_str(attr: Attributes) -> &'static str {
    match attr {
        Attributes::UNDERLINE => "underline",
        Attributes::CURLY_UNDERLINE => "curly_underline",
        Attributes::DOUBLE_UNDERLINE => "double_underline",
        Attributes::REVERSE => "reverse",
        Attributes::BLINK => "blink",
        Attributes::BOLD => "bold",
        Attributes::DIM => "dim",
        Attributes::ITALIC => "italic",
        Attributes::STRIKETHROUGH => "strikethrough",
        Attributes::DOTTED_UNDERLINE => "dotted_underline",
        Attributes::DASHED_UNDERLINE => "dashed_underline",
        Attributes::FINAL_FG => "final_fg",
        Attributes::FINAL_BG => "final_bg",
        Attributes::FINAL_ATTR => "final_attr",
        _ => "unknown",
    }
}

impl<'de> Deserialize<'de> for Attributes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AttrsVisitor;

        impl<'de> Visitor<'de> for AttrsVisitor {
            type Value = Attributes;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an array of attribute strings")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Attributes, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut flags = Attributes::empty();
                while let Some(s) = seq.next_element::<&str>()? {
                    flags |= parse_attribute(s)
                        .ok_or_else(|| de::Error::custom(format!("unknown attribute: {s}")))?;
                }
                Ok(flags)
            }
        }

        deserializer.deserialize_seq(AttrsVisitor)
    }
}

impl Serialize for Attributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let count = self.iter().count();
        let mut seq = serializer.serialize_seq(Some(count))?;
        for flag in self.iter() {
            seq.serialize_element(attribute_str(flag))?;
        }
        seq.end()
    }
}

// ---------------------------------------------------------------------------
// Face
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub fg: Color,
    pub bg: Color,
    pub underline: Color,
    pub attributes: Attributes,
}

impl Default for Face {
    fn default() -> Self {
        Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Attributes::empty(),
        }
    }
}

impl<'de> Deserialize<'de> for Face {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FaceHelper {
            fg: Color,
            bg: Color,
            #[serde(default)]
            underline: Color,
            #[serde(default)]
            attributes: Attributes,
        }

        let h = FaceHelper::deserialize(deserializer)?;
        Ok(Face {
            fg: h.fg,
            bg: h.bg,
            underline: h.underline,
            attributes: h.attributes,
        })
    }
}

impl Serialize for Face {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Face", 4)?;
        s.serialize_field("fg", &self.fg)?;
        s.serialize_field("bg", &self.bg)?;
        s.serialize_field("underline", &self.underline)?;
        s.serialize_field("attributes", &self.attributes)?;
        s.end()
    }
}

// ---------------------------------------------------------------------------
// Face resolution
// ---------------------------------------------------------------------------

/// Resolve Default colors in an atom face against a base face.
/// In Kakoune, `default` means "inherit from the containing context".
pub fn resolve_face(atom_face: &Face, base: &Face) -> Face {
    Face {
        fg: if atom_face.fg == Color::Default {
            base.fg
        } else {
            atom_face.fg
        },
        bg: if atom_face.bg == Color::Default {
            base.bg
        } else {
            atom_face.bg
        },
        underline: if atom_face.underline == Color::Default {
            base.underline
        } else {
            atom_face.underline
        },
        attributes: if atom_face.attributes.contains(Attributes::FINAL_ATTR)
            || base.attributes.is_empty()
        {
            atom_face.attributes
        } else {
            base.attributes | atom_face.attributes
        },
    }
}
