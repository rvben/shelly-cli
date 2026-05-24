# `shelly light` RGB Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `shelly light` command that controls Gen2/Gen3 light-output devices (RGB, RGBW, CCT, dimmable) with color, brightness, white, and color-temperature flags.

**Architecture:** A new pure color-parsing module (`src/color.rs`) and light model (`src/model/light.rs`) feed a `cmd_light` handler that mirrors the existing `cmd_switch` flow. Capability detection reads `Shelly.GetStatus` live, validates `--id` and flags against the detected component kind, then issues the matching `*.Set`/`*.Toggle`/`*.GetStatus` RPC via `Gen2Device`. Gen1 returns a clear "not yet supported" error.

**Tech Stack:** Rust 2024, clap (derive), serde_json, reqwest, anyhow. Tests via `cargo nextest run`.

**Spec:** `docs/superpowers/specs/2026-05-24-shelly-light-rgb-design.md`

---

## File Structure

- `src/color.rs` (new) — `Rgb` struct, `parse_color`, `parse_rgb_triple`. Pure, no I/O.
- `src/model/light.rs` (new) — `LightKind`, `LightComponent` (+ `from_status`), `LightStatus` (+ `from_component_json`), `LightParams`, capability helpers. Pure.
- `src/model/mod.rs` (modify) — declare and re-export the `light` module types.
- `src/api/gen2.rs` (modify) — `light_components`, `light_set`, `light_toggle`, `light_status`, plus private `build_set_body`.
- `src/api/mod.rs` (modify) — `ShellyDevice` dispatch for the four light methods; Gen1 returns the "not supported" error.
- `src/cli/mod.rs` (modify) — `Light` command + `LightAction` subcommands and flags.
- `src/main.rs` (modify) — `mod color;`, `Command::Light` dispatch, `cmd_light`, `validate_light_id`.
- `src/output.rs` (modify) — `print_light_status`.
- `src/schema.rs` (modify) — add light mutating commands to `mutating_commands`.
- `README.md` (modify) — usage docs.

---

## Task 1: Color parsing module

**Files:**
- Create: `src/color.rs`
- Modify: `src/main.rs:11` (add `mod color;` after `mod cache;`)

- [ ] **Step 1: Create the module with failing tests**

Create `src/color.rs`:

```rust
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
        p.parse::<u8>()
            .map_err(|_| anyhow::anyhow!("invalid --rgb component '{p}'. Each value must be 0..255"))
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
        assert_eq!(parse_color("#00ff88").unwrap(), Rgb { r: 0, g: 255, b: 136 });
        assert_eq!(parse_color("00FF88").unwrap(), Rgb { r: 0, g: 255, b: 136 });
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
        assert_eq!(parse_rgb_triple("0,255,136").unwrap(), Rgb { r: 0, g: 255, b: 136 });
        assert_eq!(parse_rgb_triple(" 1 , 2 , 3 ").unwrap(), Rgb { r: 1, g: 2, b: 3 });
        assert!(parse_rgb_triple("0,255").is_err());
        assert!(parse_rgb_triple("0,256,0").is_err());
        assert!(parse_rgb_triple("-1,0,0").is_err());
    }

    #[test]
    fn to_array_order() {
        assert_eq!(Rgb { r: 1, g: 2, b: 3 }.to_array(), [1, 2, 3]);
    }
}
```

Add `mod color;` in `src/main.rs` immediately after the existing `mod cache;` line (`src/main.rs:2`).

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo nextest run color::`
Expected: PASS (5 tests). If `mod color;` is missing the crate won't compile.

- [ ] **Step 3: Verify lint is clean**

Run: `make lint`
Expected: no warnings/errors from `src/color.rs`.

- [ ] **Step 4: Commit**

```bash
git add src/color.rs src/main.rs
git commit -m "feat(light): add color parsing module"
```

---

## Task 2: Light model (kinds, components, status, params)

**Files:**
- Create: `src/model/light.rs`
- Modify: `src/model/mod.rs`

- [ ] **Step 1: Create the model with failing tests**

Create `src/model/light.rs`:

```rust
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
```

In `src/model/mod.rs` add the module declaration alongside the existing ones and re-export the types. Match the existing style in that file (it declares submodules like `pub mod status;` / `pub mod device;` and re-exports). Add:

```rust
pub mod light;
pub use light::{LightComponent, LightKind, LightParams, LightStatus};
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo nextest run model::light`
Expected: PASS (4 tests).

- [ ] **Step 3: Verify lint is clean**

Run: `make lint`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/model/light.rs src/model/mod.rs
git commit -m "feat(light): add light model, detection, and status parsing"
```

---

## Task 3: Gen2 RPC methods + Set body builder

**Files:**
- Modify: `src/api/gen2.rs`

- [ ] **Step 1: Add the body builder and methods with failing tests**

At the top of `src/api/gen2.rs`, extend the `use crate::model::...` import to include the light types:

```rust
use crate::model::{DeviceInfo, DeviceStatus, LightComponent, LightKind, LightParams, LightStatus, PowerReading, SwitchStatus};
```

Add this private free function near the bottom of the file, before the `#[cfg(test)]` module (or at end of file if none):

```rust
/// Build the JSON body for a `<Kind>.Set` call from light params. Only the
/// fields relevant to the component kind and present in `params` are included.
/// Always includes `id`.
fn build_set_body(kind: LightKind, id: u8, params: &LightParams) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("id".to_string(), serde_json::json!(id));
    if let Some(on) = params.on {
        body.insert("on".to_string(), serde_json::json!(on));
    }
    if let Some(b) = params.brightness {
        body.insert("brightness".to_string(), serde_json::json!(b));
    }
    if kind.supports_rgb()
        && let Some(rgb) = params.rgb
    {
        body.insert("rgb".to_string(), serde_json::json!(rgb));
    }
    if kind.supports_white()
        && let Some(w) = params.white
    {
        body.insert("white".to_string(), serde_json::json!(w));
    }
    if kind.supports_ct()
        && let Some(ct) = params.ct
    {
        body.insert("ct".to_string(), serde_json::json!(ct));
    }
    serde_json::Value::Object(body)
}
```

Add these methods inside `impl Gen2Device` (after `switch_toggle`, around `src/api/gen2.rs:97`):

```rust
    pub async fn light_components(&self) -> Result<Vec<LightComponent>> {
        let status = self.rpc_call("Shelly.GetStatus", None).await?;
        Ok(LightComponent::from_status(&status))
    }

    pub async fn light_set(
        &self,
        kind: LightKind,
        id: u8,
        params: &LightParams,
    ) -> Result<SwitchResult> {
        let body = build_set_body(kind, id, params);
        let method = format!("{}.Set", kind.rpc_namespace());
        let resp = self.rpc_call(&method, Some(body)).await?;
        let was_on = resp
            .get("was_on")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(SwitchResult { was_on })
    }

    pub async fn light_toggle(&self, kind: LightKind, id: u8) -> Result<SwitchResult> {
        let method = format!("{}.Toggle", kind.rpc_namespace());
        let resp = self
            .rpc_call(&method, Some(serde_json::json!({ "id": id })))
            .await?;
        let was_on = resp
            .get("was_on")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(SwitchResult { was_on })
    }

    pub async fn light_status(&self, kind: LightKind, id: u8) -> Result<LightStatus> {
        let method = format!("{}.GetStatus", kind.rpc_namespace());
        let resp = self
            .rpc_call(&method, Some(serde_json::json!({ "id": id })))
            .await?;
        Ok(LightStatus::from_component_json(kind, id, &resp))
    }
```

`SwitchResult` is already imported via `use super::{FirmwareInfo, SwitchResult};` at the top of the file.

Add a test module at the end of `src/api/gen2.rs` (if one already exists, append these tests to it instead):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LightKind, LightParams};

    #[test]
    fn rgb_set_body_includes_color_and_brightness() {
        let params = LightParams {
            on: Some(true),
            rgb: Some([0, 255, 136]),
            brightness: Some(80),
            ..Default::default()
        };
        let body = build_set_body(LightKind::Rgb, 0, &params);
        assert_eq!(
            body,
            serde_json::json!({ "id": 0, "on": true, "brightness": 80, "rgb": [0, 255, 136] })
        );
    }

    #[test]
    fn rgb_body_omits_white_and_ct() {
        let params = LightParams {
            on: Some(true),
            rgb: Some([1, 2, 3]),
            white: Some(50),
            ct: Some(3000),
            ..Default::default()
        };
        let body = build_set_body(LightKind::Rgb, 0, &params);
        assert!(body.get("white").is_none());
        assert!(body.get("ct").is_none());
    }

    #[test]
    fn rgbw_body_includes_white() {
        let params = LightParams {
            on: Some(true),
            rgb: Some([1, 2, 3]),
            white: Some(255),
            ..Default::default()
        };
        let body = build_set_body(LightKind::Rgbw, 1, &params);
        assert_eq!(body.get("white"), Some(&serde_json::json!(255)));
        assert_eq!(body.get("id"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn cct_body_includes_ct_not_rgb() {
        let params = LightParams {
            on: Some(false),
            ct: Some(3000),
            rgb: Some([1, 2, 3]),
            ..Default::default()
        };
        let body = build_set_body(LightKind::Cct, 0, &params);
        assert_eq!(body.get("ct"), Some(&serde_json::json!(3000)));
        assert!(body.get("rgb").is_none());
    }

    #[test]
    fn set_preserving_power_carries_current_on() {
        // `set` builds params with on = current state; verify it lands in the body.
        let params = LightParams {
            on: Some(true),
            brightness: Some(40),
            ..Default::default()
        };
        let body = build_set_body(LightKind::Light, 0, &params);
        assert_eq!(body.get("on"), Some(&serde_json::json!(true)));
        assert_eq!(body.get("brightness"), Some(&serde_json::json!(40)));
    }
}
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo nextest run -p shelly-cli gen2::tests`
Expected: PASS (5 tests).

- [ ] **Step 3: Verify lint is clean**

Run: `make lint`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/api/gen2.rs
git commit -m "feat(light): add Gen2 light Set/Toggle/GetStatus RPC methods"
```

---

## Task 4: ShellyDevice dispatch + Gen1 unsupported path

**Files:**
- Modify: `src/api/mod.rs`

- [ ] **Step 1: Add dispatch methods**

Extend the import at `src/api/mod.rs:9`:

```rust
use crate::model::{DeviceInfo, DeviceStatus, LightComponent, LightKind, LightParams, LightStatus, PowerReading, SwitchStatus};
```

Add these methods inside `impl ShellyDevice` (after `switch_toggle`, around `src/api/mod.rs:63`). Gen1 returns the planned-feature error so the failure surfaces at detection time:

```rust
    pub async fn light_components(&self) -> Result<Vec<LightComponent>> {
        match self {
            Self::Gen1(_) => {
                anyhow::bail!("light control for Gen1 devices is not yet implemented (planned)")
            }
            Self::Gen2(d) => d.light_components().await,
        }
    }

    pub async fn light_set(
        &self,
        kind: LightKind,
        id: u8,
        params: &LightParams,
    ) -> Result<SwitchResult> {
        match self {
            Self::Gen1(_) => {
                anyhow::bail!("light control for Gen1 devices is not yet implemented (planned)")
            }
            Self::Gen2(d) => d.light_set(kind, id, params).await,
        }
    }

    pub async fn light_toggle(&self, kind: LightKind, id: u8) -> Result<SwitchResult> {
        match self {
            Self::Gen1(_) => {
                anyhow::bail!("light control for Gen1 devices is not yet implemented (planned)")
            }
            Self::Gen2(d) => d.light_toggle(kind, id).await,
        }
    }

    pub async fn light_status(&self, kind: LightKind, id: u8) -> Result<LightStatus> {
        match self {
            Self::Gen1(_) => {
                anyhow::bail!("light control for Gen1 devices is not yet implemented (planned)")
            }
            Self::Gen2(d) => d.light_status(kind, id).await,
        }
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: builds with no errors.

- [ ] **Step 3: Verify lint is clean**

Run: `make lint`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/api/mod.rs
git commit -m "feat(light): dispatch light methods through ShellyDevice; Gen1 returns unsupported"
```

---

## Task 5: CLI command and subcommands

**Files:**
- Modify: `src/cli/mod.rs`

- [ ] **Step 1: Add the `Light` command variant**

In the `Command` enum in `src/cli/mod.rs`, add a `Light` variant next to `Switch` (after the `Switch { ... }` block, around `src/cli/mod.rs:78`):

```rust
    /// Control RGB / RGBW / CCT / dimmable light outputs (Gen2/Gen3)
    Light {
        #[command(subcommand)]
        action: LightAction,
    },
```

- [ ] **Step 2: Add the `LightAction` enum**

Add after the `SwitchAction` enum (around `src/cli/mod.rs:239`). The color flags repeat on `On` and `Set`; both are spelled out in full (the engineer may read tasks out of order):

```rust
#[derive(Subcommand, Clone)]
pub enum LightAction {
    /// Show light status (on/off, color, brightness)
    Status {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Turn light on, optionally setting color/brightness/white/temp
    On {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
        /// Color as hex (#00ff88) or name (red, green, warm, ...)
        #[arg(long, conflicts_with = "rgb")]
        color: Option<String>,
        /// Color as comma-separated r,g,b (each 0-255), e.g. 0,255,136
        #[arg(long)]
        rgb: Option<String>,
        /// Brightness 1-100 (RGB/RGBW) or 0-100 (CCT/dimmable)
        #[arg(long)]
        brightness: Option<u8>,
        /// White channel 0-255 (RGBW only)
        #[arg(long)]
        white: Option<u8>,
        /// Color temperature in Kelvin (CCT only)
        #[arg(long)]
        temp: Option<u32>,
    },
    /// Turn light off
    Off {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Toggle light on/off
    Toggle {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
    },
    /// Change attributes without changing power state
    Set {
        /// Light component ID (default: 0)
        #[arg(long, default_value = "0")]
        id: u8,
        /// Color as hex (#00ff88) or name (red, green, warm, ...)
        #[arg(long, conflicts_with = "rgb")]
        color: Option<String>,
        /// Color as comma-separated r,g,b (each 0-255), e.g. 0,255,136
        #[arg(long)]
        rgb: Option<String>,
        /// Brightness 1-100 (RGB/RGBW) or 0-100 (CCT/dimmable)
        #[arg(long)]
        brightness: Option<u8>,
        /// White channel 0-255 (RGBW only)
        #[arg(long)]
        white: Option<u8>,
        /// Color temperature in Kelvin (CCT only)
        #[arg(long)]
        temp: Option<u32>,
    },
}
```

- [ ] **Step 3: Verify it compiles (the match in main.rs is not yet exhaustive — expect a failure here)**

Run: `cargo build`
Expected: FAIL — `Command::Light` is not handled in the `match cli.command` in `src/main.rs`. This is fixed in Task 6. (If you prefer a clean build between tasks, do Task 5 and Task 6 back-to-back before building.)

- [ ] **Step 4: Commit**

```bash
git add src/cli/mod.rs
git commit -m "feat(light): add light CLI command and subcommands"
```

---

## Task 6: `cmd_light` handler + `validate_light_id`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire imports and dispatch**

In `src/main.rs`, extend the CLI import (`src/main.rs:24`) to include `LightAction`:

```rust
    Cli, Command, ConfigAction, FirmwareAction, GroupAction, LightAction, ScheduleAction, SwitchAction,
```

Add light type imports where the other `model` types are imported near the top of `src/main.rs` (find the existing `use model::{...}` / `use crate::model::...`; add `LightComponent, LightKind, LightParams`). If model types are referenced via the `model::` path elsewhere, use `model::LightKind` etc. consistently — match the file's existing convention.

Add a dispatch arm in the `match cli.command` block, next to the `Command::Switch` arm (`src/main.rs:92`):

```rust
        Command::Light { ref action } => {
            cmd_light(&cli, &http_client, &password, action.clone(), json_output).await
        }
```

- [ ] **Step 2: Add `validate_light_id` with a failing test**

Add this function near `validate_switch_id` (`src/main.rs:822`):

```rust
/// Validate that a light component ID exists on the device and return its kind.
fn validate_light_id(
    components: &[model::LightComponent],
    id: u8,
    device_name: &str,
) -> Result<model::LightKind> {
    if let Some(c) = components.iter().find(|c| c.id == id) {
        return Ok(c.kind);
    }
    if components.is_empty() {
        anyhow::bail!(
            "{device_name} has no RGB/light outputs. 'light' supports Gen2/Gen3 RGB, RGBW, CCT, and dimmable devices."
        );
    }
    let ids: Vec<String> = components.iter().map(|c| c.id.to_string()).collect();
    anyhow::bail!(
        "light ID {id} is out of range for {device_name} (valid IDs: {})",
        ids.join(", ")
    );
}
```

Add (or extend) a `#[cfg(test)]` module at the end of `src/main.rs`:

```rust
#[cfg(test)]
mod light_tests {
    use super::*;
    use model::{LightComponent, LightKind};

    #[test]
    fn validate_light_id_returns_kind() {
        let comps = vec![LightComponent { kind: LightKind::Rgb, id: 0 }];
        assert_eq!(validate_light_id(&comps, 0, "Lamp").unwrap(), LightKind::Rgb);
    }

    #[test]
    fn validate_light_id_no_components_errors() {
        let err = validate_light_id(&[], 0, "Switch1").unwrap_err().to_string();
        assert!(err.contains("no RGB/light outputs"));
    }

    #[test]
    fn validate_light_id_out_of_range_errors() {
        let comps = vec![LightComponent { kind: LightKind::Rgb, id: 0 }];
        let err = validate_light_id(&comps, 2, "Lamp").unwrap_err().to_string();
        assert!(err.contains("out of range"));
        assert!(err.contains("valid IDs: 0"));
    }
}
```

- [ ] **Step 3: Add flag validation + param building helper**

Add this helper near `cmd_light` (defined in Step 4). It enforces the spec's applicability rules and per-kind brightness bounds, then builds `LightParams` (without `on`, which the caller sets):

```rust
/// Validate flags against the component kind and build params (color/brightness/
/// white/temp). The caller sets `on` separately.
fn build_light_params(
    kind: model::LightKind,
    device_name: &str,
    color: &Option<String>,
    rgb: &Option<String>,
    brightness: Option<u8>,
    white: Option<u8>,
    temp: Option<u32>,
) -> Result<model::LightParams> {
    let mut params = model::LightParams::default();

    if (color.is_some() || rgb.is_some()) && !kind.supports_rgb() {
        anyhow::bail!(
            "color is not supported on {device_name}'s {} output; use --brightness{}",
            kind.as_str(),
            if kind.supports_ct() { " or --temp" } else { "" }
        );
    }
    if white.is_some() && !kind.supports_white() {
        anyhow::bail!(
            "--white is only valid for RGBW lights; {device_name} has a {} output",
            kind.as_str()
        );
    }
    if temp.is_some() && !kind.supports_ct() {
        anyhow::bail!(
            "--temp is only valid for color-temperature (cct) lights; {device_name} has a {} output",
            kind.as_str()
        );
    }

    if let Some(c) = color {
        params.rgb = Some(color::parse_color(c)?.to_array());
    } else if let Some(t) = rgb {
        params.rgb = Some(color::parse_rgb_triple(t)?.to_array());
    }

    if let Some(b) = brightness {
        let min = kind.brightness_min();
        if b < min || b > 100 {
            anyhow::bail!(
                "--brightness for {} lights must be {min}-100, got {b}",
                kind.as_str()
            );
        }
        params.brightness = Some(b);
    }

    params.white = white;
    params.ct = temp;
    Ok(params)
}
```

- [ ] **Step 4: Add `cmd_light`**

Add this handler after `cmd_switch` (`src/main.rs:582`). It mirrors `cmd_switch`'s structure (resolve targets, per-device JSON accumulation, final `print_json_success`):

```rust
async fn cmd_light(
    cli: &Cli,
    http_client: &reqwest::Client,
    password: &Option<String>,
    action: LightAction,
    json_output: bool,
) -> Result<()> {
    let targets = resolve_and_probe_targets(cli, http_client, password).await?;
    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for device in &targets {
        let name = device.info().display_name().to_string();
        let components = device.light_components().await?;

        let id = match &action {
            LightAction::Status { id }
            | LightAction::On { id, .. }
            | LightAction::Off { id }
            | LightAction::Toggle { id }
            | LightAction::Set { id, .. } => *id,
        };
        let kind = validate_light_id(&components, id, &name)?;

        match &action {
            LightAction::Status { .. } => {
                let status = device.light_status(kind, id).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "status": status }));
                } else {
                    if targets.len() > 1 {
                        print!("{name}: ");
                    }
                    output::print_light_status(&status);
                }
            }
            LightAction::On { color, rgb, brightness, white, temp, .. } => {
                let mut params =
                    build_light_params(kind, &name, color, rgb, *brightness, *white, *temp)?;
                params.on = Some(true);
                let result = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let on_label = colored_on_off(true, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Light {id} {on_label} (was {was_label})");
                }
            }
            LightAction::Off { .. } => {
                let params = model::LightParams { on: Some(false), ..Default::default() };
                let result = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let off_label = colored_on_off(false, !json_output);
                    let was_label = colored_on_off(result.was_on, !json_output);
                    println!("{name}: Light {id} {off_label} (was {was_label})");
                }
            }
            LightAction::Toggle { .. } => {
                let result = device.light_toggle(kind, id).await?;
                if json_output {
                    json_results
                        .push(serde_json::json!({ "device": name, "was_on": result.was_on }));
                } else {
                    let was_label = colored_on_off(result.was_on, !json_output);
                    let toggled = if output::use_color() {
                        "TOGGLED".cyan().to_string()
                    } else {
                        "TOGGLED".to_string()
                    };
                    println!("{name}: Light {id} {toggled} (was {was_label})");
                }
            }
            LightAction::Set { color, rgb, brightness, white, temp, .. } => {
                let mut params =
                    build_light_params(kind, &name, color, rgb, *brightness, *white, *temp)?;
                if params.rgb.is_none()
                    && params.brightness.is_none()
                    && params.white.is_none()
                    && params.ct.is_none()
                {
                    anyhow::bail!(
                        "light set requires at least one of --color/--rgb, --brightness, --white, --temp"
                    );
                }
                // Preserve power: send the current on-state so the device's
                // "at least one of on/brightness" requirement is satisfied.
                let current = device.light_status(kind, id).await?;
                params.on = Some(current.output);
                let _ = device.light_set(kind, id, &params).await?;
                if json_output {
                    json_results.push(serde_json::json!({ "device": name, "id": id }));
                } else {
                    println!("{name}: Light {id} updated");
                }
            }
        }
    }

    if json_output {
        output::print_json_success(&json_results);
    }
    Ok(())
}
```

`.cyan()` and `colored_on_off` are already in scope in `main.rs` (used by `cmd_switch`).

- [ ] **Step 5: Run tests and build**

Run: `cargo nextest run light_tests`
Expected: PASS (3 tests).

Run: `cargo build`
Expected: builds clean (Task 5's `Command::Light` is now handled).

- [ ] **Step 6: Verify lint and full test suite**

Run: `make lint && cargo nextest run`
Expected: no warnings; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat(light): add cmd_light handler with flag validation and id checks"
```

---

## Task 7: Light status output rendering

**Files:**
- Modify: `src/output.rs`

- [ ] **Step 1: Add `print_light_status`**

Add after `print_switch_status` (`src/output.rs:311`). Match the existing indentation/label style:

```rust
pub fn print_light_status(s: &crate::model::LightStatus) {
    let color = use_color();
    let state = if s.output {
        if color { "ON".green().to_string() } else { "ON".to_string() }
    } else if color {
        "OFF".dimmed().to_string()
    } else {
        "OFF".to_string()
    };
    println!("  Light {} ({}): {state}", s.id, s.kind);
    if let Some([r, g, b]) = s.rgb {
        println!("    Color: #{r:02x}{g:02x}{b:02x} (rgb {r},{g},{b})");
    }
    if let Some(w) = s.white {
        println!("    White: {w}");
    }
    if let Some(ct) = s.ct {
        println!("    Temp: {ct:.0}K");
    }
    if let Some(br) = s.brightness {
        println!("    Brightness: {br:.0}%");
    }
}
```

Confirm `print_light_status` is reachable from `main.rs` as `output::print_light_status` (it is `pub fn` in the `output` module, same as `print_switch_status`).

- [ ] **Step 2: Build and lint**

Run: `cargo build && make lint`
Expected: builds clean, no warnings.

- [ ] **Step 3: Commit**

```bash
git add src/output.rs
git commit -m "feat(light): render light status output"
```

---

## Task 8: Register mutating commands in schema

**Files:**
- Modify: `src/schema.rs`

- [ ] **Step 1: Add a failing test**

Add (or extend) a `#[cfg(test)]` module at the end of `src/schema.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::generate_schema;

    #[test]
    fn light_mutating_commands_are_marked() {
        let schema = generate_schema();
        let cmds = &schema["commands"];
        for name in ["light on", "light off", "light toggle", "light set"] {
            assert_eq!(
                cmds[name]["mutating"], serde_json::json!(true),
                "{name} should be mutating"
            );
        }
        assert_eq!(
            cmds["light status"]["mutating"], serde_json::json!(false),
            "light status should be read-only"
        );
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo nextest run schema::tests::light_mutating_commands_are_marked`
Expected: FAIL — `light on`/etc. currently report `mutating: false`.

- [ ] **Step 3: Add the commands to the allowlist**

In `src/schema.rs`, extend the `mutating_commands` array (`src/schema.rs:14`) with the four light commands, alongside the existing `"switch on"` entries:

```rust
        "switch on",
        "switch off",
        "switch toggle",
        "light on",
        "light off",
        "light toggle",
        "light set",
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo nextest run schema::tests::light_mutating_commands_are_marked`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `make lint`

```bash
git add src/schema.rs
git commit -m "feat(light): mark light mutating commands in schema"
```

---

## Task 9: Documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a Light control section**

In `README.md`, after the "Device control" section (the block ending around `README.md:90` with the multi-channel `--id` examples), add:

```markdown
### Light control (Gen2/Gen3 RGB / RGBW / CCT / dimmable)

```bash
shelly light on  -n "Desk Lamp" --color '#00ff88'        # RGB color (hex)
shelly light on  -n "Desk Lamp" --color warm             # named color
shelly light on  -n "Desk Lamp" --rgb 0,255,136 --brightness 80
shelly light set -n "Desk Lamp" --brightness 40          # change brightness, keep power state
shelly light on  -n "Strip" --rgb 255,0,0 --white 0      # RGBW: color + white channel
shelly light on  -n "Bulb" --temp 3000 --brightness 60   # CCT: color temperature (Kelvin)
shelly light off -n "Desk Lamp"
shelly light toggle -n "Desk Lamp"
shelly light status -n "Desk Lamp"
```

`--id` selects the component on multi-light devices (default `0`). Brightness is
1-100 for RGB/RGBW and 0-100 for CCT/dimmable. `--color` accepts hex (`#rrggbb`) or
a name (red, green, blue, white, warm, cyan, magenta, yellow, orange, purple, pink,
off); `--rgb` takes `r,g,b` each 0-255.
```

Also update the Features list (`README.md:7-25`) by adding a bullet:

```markdown
- RGB / RGBW / CCT / dimmable light control (`shelly light`, Gen2/Gen3)
```

- [ ] **Step 2: Lint the README**

Run: `rumdl check README.md` (if available) or visually confirm fenced code blocks are balanced.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document shelly light command"
```

---

## Final verification

- [ ] **Run the full suite and lint**

Run: `make lint && cargo nextest run`
Expected: no warnings; all tests pass.

- [ ] **Build the binary and check help renders**

Run: `cargo build && ./target/debug/shelly light --help && ./target/debug/shelly light on --help`
Expected: subcommands and flags (`--color`, `--rgb`, `--brightness`, `--white`, `--temp`, `--id`) display correctly.

- [ ] **Confirm schema reflects the new commands**

Run: `./target/debug/shelly schema | jq '.commands | keys[] | select(startswith("light"))'`
Expected: `light on`, `light off`, `light toggle`, `light set`, `light status` present.

> **Live hardware note:** no Gen2/Gen3 RGB device is available in the local cache, so
> the `*.Set` round-trip cannot be exercised against real hardware here. Logic is
> covered by unit tests; live verification waits for RGB hardware (the issue reporter
> may help validate). Do not claim the round-trip was hardware-tested.

---

## Spec coverage check

- Target devices (rgb/rgbw/cct/light) → Tasks 2, 3.
- Command surface (on/off/toggle/set/status, `--id`) → Tasks 5, 6.
- Color module (hex, named, rgb triple, ranges) → Task 1.
- Flag applicability + per-kind brightness bounds → Task 6 (`build_light_params`).
- `set` carries current `on` state → Task 6.
- Capability detection (live `Shelly.GetStatus`) → Tasks 2, 3.
- RPC layer (`RGB/RGBW/CCT/Light.Set/Toggle/GetStatus`) → Tasks 3, 4.
- Gen1 "not supported" → Task 4.
- Error handling (no component, wrong flag, out-of-range id, Gen1) → Tasks 4, 6.
- Schema mutating registration → Task 8.
- Docs → Task 9.
- Testing (color, detection, body construction, validate_light_id, schema) → Tasks 1, 2, 3, 6, 8.
