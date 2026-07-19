use anyhow::{Result, bail};

/// An 8-bit RGB color, matching Shelly's `rgb: [r,g,b]` (each 0..255) field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn to_array(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

/// Map a named color to its RGB value. Returns None for unknown names.
fn named_color(name: &str) -> Option<Rgb> {
    let rgb = |r, g, b| Rgb { r, g, b };
    Some(match name {
        "red" => rgb(255, 0, 0),
        "green" => rgb(0, 255, 0),
        "blue" => rgb(0, 0, 255),
        "white" => rgb(255, 255, 255),
        "warm" => rgb(255, 147, 41),
        "cyan" => rgb(0, 255, 255),
        "magenta" => rgb(255, 0, 255),
        "yellow" => rgb(255, 255, 0),
        "orange" => rgb(255, 165, 0),
        "purple" => rgb(128, 0, 128),
        "pink" => rgb(255, 192, 203),
        "off" | "black" => rgb(0, 0, 0),
        _ => return None,
    })
}

/// Parse a `--color` spec: 6-digit hex (`#00ff88` or `00ff88`) or a named color.
pub fn parse_color(spec: &str) -> Result<Rgb> {
    let s = spec.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
        return Ok(Rgb { r, g, b });
    }
    if let Some(rgb) = named_color(&s.to_lowercase()) {
        return Ok(rgb);
    }
    bail!(
        "invalid color '{spec}'. Use a hex value like '#00ff88' or a name (red, green, blue, white, warm, cyan, magenta, yellow, orange, purple, pink, off)"
    )
}

/// Parse a `--rgb` spec: comma-separated `r,g,b`, each 0..255.
pub fn parse_rgb_triple(spec: &str) -> Result<Rgb> {
    let parts: Vec<&str> = spec.split(',').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        bail!("invalid --rgb '{spec}'. Expected three comma-separated values, e.g. '0,255,136'");
    }
    let parse_one = |p: &str| -> Result<u8> {
        p.parse::<u8>().map_err(|_| {
            anyhow::anyhow!("invalid --rgb component '{p}'. Each value must be 0..255")
        })
    };
    Ok(Rgb {
        r: parse_one(parts[0])?,
        g: parse_one(parts[1])?,
        b: parse_one(parts[2])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_with_and_without_hash() {
        assert_eq!(
            parse_color("#00ff88").unwrap(),
            Rgb {
                r: 0,
                g: 255,
                b: 136
            }
        );
        assert_eq!(
            parse_color("00FF88").unwrap(),
            Rgb {
                r: 0,
                g: 255,
                b: 136
            }
        );
    }

    #[test]
    fn named_colors_resolve() {
        assert_eq!(parse_color("red").unwrap(), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(parse_color("OFF").unwrap(), Rgb { r: 0, g: 0, b: 0 });
    }

    #[test]
    fn invalid_color_errors() {
        assert!(parse_color("nope").is_err());
        assert!(parse_color("#12345").is_err());
        assert!(parse_color("gggggg").is_err());
    }

    #[test]
    fn rgb_triple_parses_and_bounds_check() {
        assert_eq!(
            parse_rgb_triple("0,255,136").unwrap(),
            Rgb {
                r: 0,
                g: 255,
                b: 136
            }
        );
        assert_eq!(
            parse_rgb_triple(" 1 , 2 , 3 ").unwrap(),
            Rgb { r: 1, g: 2, b: 3 }
        );
        assert!(parse_rgb_triple("0,255").is_err());
        assert!(parse_rgb_triple("0,256,0").is_err());
        assert!(parse_rgb_triple("-1,0,0").is_err());
    }

    #[test]
    fn to_array_order() {
        assert_eq!(Rgb { r: 1, g: 2, b: 3 }.to_array(), [1, 2, 3]);
    }
}
