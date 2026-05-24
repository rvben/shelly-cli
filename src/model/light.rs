use serde::Serialize;

/// The four Gen2/Gen3 light-output component kinds `shelly light` controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Rgb,
    Rgbw,
    Cct,
    Light,
}

impl LightKind {
    /// Component key prefix as it appears in `Shelly.GetStatus` (e.g. "rgb").
    pub fn as_str(self) -> &'static str {
        match self {
            LightKind::Rgb => "rgb",
            LightKind::Rgbw => "rgbw",
            LightKind::Cct => "cct",
            LightKind::Light => "light",
        }
    }

    /// RPC method namespace (e.g. "RGB" for "RGB.Set").
    pub fn rpc_namespace(self) -> &'static str {
        match self {
            LightKind::Rgb => "RGB",
            LightKind::Rgbw => "RGBW",
            LightKind::Cct => "CCT",
            LightKind::Light => "Light",
        }
    }

    pub fn supports_rgb(self) -> bool {
        matches!(self, LightKind::Rgb | LightKind::Rgbw)
    }

    pub fn supports_white(self) -> bool {
        matches!(self, LightKind::Rgbw)
    }

    pub fn supports_ct(self) -> bool {
        matches!(self, LightKind::Cct)
    }

    /// Minimum accepted brightness: RGB/RGBW require 1, CCT/Light allow 0.
    pub fn brightness_min(self) -> u8 {
        match self {
            LightKind::Rgb | LightKind::Rgbw => 1,
            LightKind::Cct | LightKind::Light => 0,
        }
    }
}

/// A detected light component: its kind and instance id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightComponent {
    pub kind: LightKind,
    pub id: u8,
}

impl LightComponent {
    /// Detect light components from a `Shelly.GetStatus` response by scanning
    /// keys of the form "<kind>:<id>" (rgb, rgbw, cct, light). Sorted by kind
    /// then id for stable output.
    pub fn from_status(status: &serde_json::Value) -> Vec<LightComponent> {
        let kinds = [
            ("rgb", LightKind::Rgb),
            ("rgbw", LightKind::Rgbw),
            ("cct", LightKind::Cct),
            ("light", LightKind::Light),
        ];
        let mut out = Vec::new();
        if let Some(obj) = status.as_object() {
            for key in obj.keys() {
                let Some((prefix, id_str)) = key.split_once(':') else {
                    continue;
                };
                if let Some((_, kind)) = kinds.iter().find(|(p, _)| *p == prefix)
                    && let Ok(id) = id_str.parse::<u8>()
                {
                    out.push(LightComponent { kind: *kind, id });
                }
            }
        }
        out.sort_by_key(|c| (c.kind.as_str(), c.id));
        out
    }
}

/// Attributes to apply in a single `*.Set` call. Fields left `None` are omitted
/// from the RPC body and therefore unchanged on the device.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LightParams {
    pub on: Option<bool>,
    pub rgb: Option<[u8; 3]>,
    pub white: Option<u8>,
    pub brightness: Option<u8>,
    pub ct: Option<u32>,
}

/// Current state of one light component, for `shelly light status`.
///
/// `brightness` and `ct` are kept as `f64` to round-trip whatever numeric form
/// the device reports without truncation; they are only displayed, never used in
/// further arithmetic.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LightStatus {
    pub kind: String,
    pub id: u8,
    pub output: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rgb: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub white: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ct: Option<f64>,
}

impl LightStatus {
    /// Parse from a single component status object (the value of e.g. "rgb:0"
    /// in `Shelly.GetStatus`, or the body of `<Kind>.GetStatus`).
    pub fn from_component_json(kind: LightKind, id: u8, v: &serde_json::Value) -> LightStatus {
        let rgb = v.get("rgb").and_then(|a| a.as_array()).and_then(|a| {
            if a.len() == 3 {
                Some([
                    a[0].as_u64().unwrap_or(0) as u8,
                    a[1].as_u64().unwrap_or(0) as u8,
                    a[2].as_u64().unwrap_or(0) as u8,
                ])
            } else {
                None
            }
        });
        LightStatus {
            kind: kind.as_str().to_string(),
            id,
            output: v.get("output").and_then(|o| o.as_bool()).unwrap_or(false),
            brightness: v.get("brightness").and_then(|b| b.as_f64()),
            rgb,
            white: v.get("white").and_then(|w| w.as_u64()).map(|w| w as u8),
            ct: v.get("ct").and_then(|c| c.as_f64()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capability_flags() {
        assert!(LightKind::Rgb.supports_rgb());
        assert!(LightKind::Rgbw.supports_rgb());
        assert!(!LightKind::Cct.supports_rgb());
        assert!(!LightKind::Light.supports_rgb());
        assert!(LightKind::Rgbw.supports_white());
        assert!(!LightKind::Rgb.supports_white());
        assert!(LightKind::Cct.supports_ct());
        assert_eq!(LightKind::Rgb.brightness_min(), 1);
        assert_eq!(LightKind::Cct.brightness_min(), 0);
        assert_eq!(LightKind::Light.brightness_min(), 0);
    }

    #[test]
    fn detect_components_from_status() {
        let status = json!({
            "rgb:0": {},
            "rgbw:0": {},
            "cct:0": {},
            "light:0": {},
            "switch:0": {},
            "sys": {},
        });
        let comps = LightComponent::from_status(&status);
        assert_eq!(
            comps,
            vec![
                LightComponent { kind: LightKind::Cct, id: 0 },
                LightComponent { kind: LightKind::Light, id: 0 },
                LightComponent { kind: LightKind::Rgb, id: 0 },
                LightComponent { kind: LightKind::Rgbw, id: 0 },
            ]
        );
    }

    #[test]
    fn detect_no_light_components() {
        let status = json!({ "switch:0": {}, "switch:1": {}, "sys": {} });
        assert!(LightComponent::from_status(&status).is_empty());
    }

    #[test]
    fn parse_status_fields() {
        let v = json!({
            "id": 0,
            "output": true,
            "brightness": 80.0,
            "rgb": [0, 255, 136]
        });
        let s = LightStatus::from_component_json(LightKind::Rgb, 0, &v);
        assert!(s.output);
        assert_eq!(s.brightness, Some(80.0));
        assert_eq!(s.rgb, Some([0, 255, 136]));
        assert_eq!(s.white, None);
    }
}
