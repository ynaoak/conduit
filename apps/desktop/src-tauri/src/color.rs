//! Colors picked off the screen, and the formats people paste them into.
//!
//! Deliberately free of any platform code: everything here is arithmetic, so
//! it can be tested on the machine the tests happen to run on. The part that
//! reads a pixel out of the screen lives in `screen_color`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `#rrggbb`, lowercase. The form nearly every tool accepts, and the one
    /// this app hands to the clipboard by default.
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `rgb(r, g, b)` — CSS's own spelling, so it can be pasted as-is.
    pub fn rgb_css(&self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// `hsl(h, s%, l%)`, rounded to whole degrees and percent.
    ///
    /// Rounding is what a person wants pasted into a stylesheet; the exact
    /// value is still in the hex, which is what the history stores.
    pub fn hsl_css(&self) -> String {
        let (h, s, l) = self.hsl();
        format!("hsl({}, {}%, {}%)", h.round() as i64, (s * 100.0).round(), (l * 100.0).round())
    }

    /// Hue in degrees (0..360), saturation and lightness in 0..1.
    pub fn hsl(&self) -> (f64, f64, f64) {
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        let delta = max - min;

        // A gray has no hue to speak of. Reporting 0 rather than NaN keeps
        // the formatted output usable instead of printing "NaN%".
        if delta == 0.0 {
            return (0.0, 0.0, l);
        }

        let s = delta / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        // The red sector wraps below zero for hues just under 360
        (if h < 0.0 { h + 360.0 } else { h }, s, l)
    }

    /// Parse `#rrggbb` / `rrggbb` / `#rgb`. Used to read the history file
    /// back, and to accept whatever a user has on their clipboard.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let body = text.trim().trim_start_matches('#');
        let expand = |c: char| c.to_digit(16).map(|v| (v * 17) as u8);
        match body.len() {
            3 => {
                let mut it = body.chars();
                Some(Self::new(
                    expand(it.next()?)?,
                    expand(it.next()?)?,
                    expand(it.next()?)?,
                ))
            }
            6 => Some(Self::new(
                u8::from_str_radix(&body[0..2], 16).ok()?,
                u8::from_str_radix(&body[2..4], 16).ok()?,
                u8::from_str_radix(&body[4..6], 16).ok()?,
            )),
            _ => None,
        }
    }

    /// Relative luminance (WCAG), used to decide whether text drawn on this
    /// color should be black or white.
    pub fn luminance(&self) -> f64 {
        let channel = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// Black or white, whichever stays readable on this color.
    pub fn contrasting_ink(&self) -> Rgb {
        if self.luminance() > 0.179 {
            Rgb::new(0, 0, 0)
        } else {
            Rgb::new(255, 255, 255)
        }
    }
}

/// The formats a picked color can be copied in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Hex,
    Rgb,
    Hsl,
}

impl Format {
    pub fn id(self) -> &'static str {
        match self {
            Format::Hex => "hex",
            Format::Rgb => "rgb",
            Format::Hsl => "hsl",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "hex" => Some(Format::Hex),
            "rgb" => Some(Format::Rgb),
            "hsl" => Some(Format::Hsl),
            _ => None,
        }
    }

    pub fn render(self, color: Rgb) -> String {
        match self {
            Format::Hex => color.hex(),
            Format::Rgb => color.rgb_css(),
            Format::Hsl => color.hsl_css(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        for color in [
            Rgb::new(0, 0, 0),
            Rgb::new(255, 255, 255),
            Rgb::new(1, 2, 3),
            Rgb::new(0x0b, 0x0a, 0x0d),
        ] {
            assert_eq!(Rgb::parse_hex(&color.hex()), Some(color));
        }
    }

    /// The three-digit form is what people type by hand, and `#abc` means
    /// `#aabbcc` — not `#0a0b0c`, which a naive parse would produce.
    #[test]
    fn short_hex_expands_by_repeating_each_digit() {
        assert_eq!(Rgb::parse_hex("#abc"), Some(Rgb::new(0xaa, 0xbb, 0xcc)));
        assert_eq!(Rgb::parse_hex("fff"), Some(Rgb::new(255, 255, 255)));
    }

    #[test]
    fn junk_is_not_a_color() {
        for text in ["", "#", "#12345", "#gggggg", "rgb(1,2,3)", "#1234567"] {
            assert_eq!(Rgb::parse_hex(text), None, "{text} parsed");
        }
    }

    /// The three primaries sit 120° apart, and the hue of pure red is the
    /// one that wraps: computed naively it comes out as 0 or 360 depending
    /// on which branch runs.
    #[test]
    fn hues_land_where_the_color_wheel_says() {
        let cases = [
            (Rgb::new(255, 0, 0), 0.0),
            (Rgb::new(255, 255, 0), 60.0),
            (Rgb::new(0, 255, 0), 120.0),
            (Rgb::new(0, 255, 255), 180.0),
            (Rgb::new(0, 0, 255), 240.0),
            (Rgb::new(255, 0, 255), 300.0),
        ];
        for (color, expected) in cases {
            let (h, s, l) = color.hsl();
            assert!((h - expected).abs() < 0.001, "{} -> {h}", color.hex());
            assert!((s - 1.0).abs() < 0.001, "{} saturation {s}", color.hex());
            assert!((l - 0.5).abs() < 0.001, "{} lightness {l}", color.hex());
        }
    }

    /// A gray divides by a zero delta. Without the early return this prints
    /// "hsl(NaN, NaN%, 50%)", which is worse than useless in a stylesheet.
    #[test]
    fn a_gray_has_no_hue_rather_than_a_nan() {
        for value in [0u8, 64, 128, 255] {
            let (h, s, _) = Rgb::new(value, value, value).hsl();
            assert_eq!(h, 0.0);
            assert_eq!(s, 0.0);
        }
        assert_eq!(Rgb::new(128, 128, 128).hsl_css(), "hsl(0, 0%, 50%)");
    }

    #[test]
    fn formats_render_the_way_css_spells_them() {
        let color = Rgb::new(11, 10, 13);
        assert_eq!(Format::Hex.render(color), "#0b0a0d");
        assert_eq!(Format::Rgb.render(color), "rgb(11, 10, 13)");
        assert_eq!(Format::Hsl.render(color), "hsl(260, 13%, 5%)");
    }

    /// The Color Palette tool window converts in JavaScript — a tool window
    /// has no IPC, so the arithmetic exists twice. Two implementations that
    /// disagree would show one color as two different HSLs depending on
    /// which window you looked at, so these expectations were measured from
    /// the page's own output and are asserted against this side.
    #[test]
    fn the_tool_windows_conversions_agree_with_these() {
        let cases = [
            ("#ff8800", "rgb(255, 136, 0)", "hsl(32, 100%, 50%)", false),
            ("#0b0a0d", "rgb(11, 10, 13)", "hsl(260, 13%, 5%)", true),
            ("#2e8b57", "rgb(46, 139, 87)", "hsl(146, 50%, 36%)", false),
            ("#ffffff", "rgb(255, 255, 255)", "hsl(0, 0%, 100%)", false),
            ("#000000", "rgb(0, 0, 0)", "hsl(0, 0%, 0%)", true),
            ("#808080", "rgb(128, 128, 128)", "hsl(0, 0%, 50%)", false),
            ("#00ff00", "rgb(0, 255, 0)", "hsl(120, 100%, 50%)", false),
            ("#123", "rgb(17, 34, 51)", "hsl(210, 50%, 13%)", true),
        ];
        for (input, rgb, hsl, wants_white_ink) in cases {
            let color = Rgb::parse_hex(input).expect(input);
            assert_eq!(color.rgb_css(), rgb, "{input}");
            assert_eq!(color.hsl_css(), hsl, "{input}");
            let ink = color.contrasting_ink();
            assert_eq!(ink == Rgb::new(255, 255, 255), wants_white_ink, "{input} ink");
        }
    }

    /// Every format id has to survive the round trip through an action id,
    /// or the action silently falls back to the default one.
    #[test]
    fn every_format_is_reachable_by_its_id() {
        for format in [Format::Hex, Format::Rgb, Format::Hsl] {
            assert_eq!(Format::from_id(format.id()), Some(format));
        }
        assert_eq!(Format::from_id("cmyk"), None);
    }

    /// The icon is the whole point of the row: decoding it back has to
    /// produce the color that went in, or the history is a list of wrong
    /// colored squares that all look plausible.
    #[test]
    fn a_swatch_decodes_back_to_its_own_color() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let color = Rgb::new(0x2e, 0x8b, 0x57);
        let encoded = swatch_png(color).expect("swatch encodes");
        let bytes = STANDARD.decode(&encoded).expect("valid base64");

        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().expect("valid png");
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).expect("valid frame");

        assert_eq!((info.width, info.height), (32, 32));
        for pixel in buffer[..info.buffer_size()].chunks_exact(4) {
            assert_eq!(pixel, [color.r, color.g, color.b, 255]);
        }
    }

    /// The swatch draws its own hex on top of itself, so the ink has to flip
    /// somewhere in the middle of the range or one end becomes unreadable.
    #[test]
    fn ink_flips_between_a_dark_and_a_light_swatch() {
        assert_eq!(Rgb::new(0, 0, 0).contrasting_ink(), Rgb::new(255, 255, 255));
        assert_eq!(Rgb::new(255, 255, 255).contrasting_ink(), Rgb::new(0, 0, 0));
        assert_eq!(Rgb::new(255, 255, 0).contrasting_ink(), Rgb::new(0, 0, 0));
        assert_eq!(Rgb::new(0, 0, 128).contrasting_ink(), Rgb::new(255, 255, 255));
    }
}

/// A solid swatch as a base64 PNG, for `ResultIcon::Base64`.
///
/// The launcher's result rows can show a real image but not inline SVG, and
/// a color is far easier to recognise as itself than as a hex string in a
/// subtitle — so the icon is the color.
pub fn swatch_png(color: Rgb) -> Option<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    const SIZE: u32 = 32;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _ in 0..SIZE * SIZE {
        pixels.extend_from_slice(&[color.r, color.g, color.b, 255]);
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, SIZE, SIZE);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&pixels).ok()?;
    }
    Some(STANDARD.encode(png))
}
