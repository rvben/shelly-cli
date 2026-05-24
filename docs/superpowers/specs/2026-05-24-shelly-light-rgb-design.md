# Design: `shelly light` + color parsing (Gen2/Gen3 RGB output)

_Date: 2026-05-24_
_Status: Approved for planning_

## Context

This is sub-project #1 of a larger "RGB / LED control" effort tracked from issue #1
(Shelly PowerStrip 4). The full effort was decomposed into three independently
shippable pieces, to be built in confidence order:

1. **`shelly light` for Gen2/Gen3 RGB output** + shared color-parsing module (this spec).
2. Gen1 RGB output (RGBW2 `/color/0`, Bulb/Duo `/light/0`).
3. `shelly led` status-LED config (PowerStrip `powerstrip_ui`, plugs `plugs_ui`) -
   deferred last because its RPC method name is unconfirmed and no test hardware is
   available locally.

Two distinct mechanisms motivated the split:
- **Light output** devices: the LED *is* the load. Color is *live state*, set via
  `*.Set` RPCs - semantically like the existing on/off switching.
- **Status/indicator LEDs**: color is *persisted config* set via `*.SetConfig`.

This spec covers light output only. `shelly led` (status-LED config) is a separate
spec.

## Goals

- A `shelly light` command that controls Gen2/Gen3 light output devices: on/off,
  toggle, color, brightness, white channel, and color temperature.
- A reusable color-parsing module shared by this and future sub-projects.
- Behavior, validation, and errors consistent with the existing `switch` command.

## Non-goals

- Gen1 light devices (sub-project #2).
- Status/indicator LED configuration (`shelly led`, sub-project #3).
- Effects/transitions/schedules beyond a single Set call.
- Surfacing light state in `info`/`devices` listings (separate reporting work).

## Target devices and RPC mapping

Light components are detected live from `Shelly.GetStatus` (see Capability
Detection). Each component kind maps to one Set RPC:

| Component key | RPC method | Capabilities exposed |
|---------------|------------|----------------------|
| `rgb:N`       | `RGB.Set`  | on/off, `rgb`, brightness |
| `rgbw:N`      | `RGBW.Set` | on/off, `rgb`, `white`, brightness |
| `cct:N`       | `CCT.Set`  | on/off, color temperature (`ct`), brightness |
| `light:N`     | `Light.Set`| on/off, brightness only (no color) |

A Gen1 device returns a clear "not yet supported" error (sub-project #2).

## Command surface

Mirrors the existing `switch` subcommand structure.

```
shelly light on     [--id N] [--color C | --rgb r,g,b] [--brightness 0-100] [--white 0-255] [--temp K]
shelly light off    [--id N]
shelly light toggle [--id N]
shelly light set    [--id N] [color/brightness flags]   # change attributes, power unchanged
shelly light status [--id N]                             # show on/off, color, brightness
```

- `--id` defaults to `0`, validated against detected components (same pattern as
  `validate_switch_id` in `src/main.rs`).
- `on` and `set` apply all provided attributes atomically within the single `*.Set`
  call. The difference: `on` forces `on: true`; `set` preserves the current power
  state. Because every Set RPC requires "at least one of `on`/`brightness`" (see
  Confirmed RPC Ranges), `set` includes `on: <current>` read from the same
  `GetStatus` used for capability detection - this both preserves power and satisfies
  the requirement even when the user changes only color.
- `off` and `toggle` take only `--id`.
- Targeting via the existing global flags (`--host`, `--name`, `--group`) and JSON
  output behavior are inherited unchanged.

## Shared color module (`src/color.rs`)

Public surface (names indicative):

- `parse_color(spec: &str) -> Result<Rgb>` accepts:
  - Hex: `#00ff88` or `00ff88` (6 hex digits, optional leading `#`).
  - Named: `red, green, blue, white, warm, cyan, magenta, yellow, orange, purple,
    pink, off/black` (final palette decided in implementation; maps to RGB internally).
- `parse_rgb_triple(spec: &str) -> Result<Rgb>`: comma-separated `r,g,b`, each `0-255`.
- `Rgb { r: u8, g: u8, b: u8 }` with `to_array() -> [u8; 3]` for the Shelly `rgb`
  field.
- Brightness: `--brightness` validated per component kind - `1-100` for `rgb`/`rgbw`,
  `0-100` for `cct`/`light` (see Confirmed RPC Ranges).
- White: `--white` `0-255` (rgbw only).
- Temp: `--temp` Kelvin integer (cct only), passed through as `ct`. Range is
  device-specific (default e.g. 2700-6500K, configurable); not hardcoded - an
  out-of-range value surfaces the device's own error.

### Flag applicability and conflict rules

Enforced before any RPC call, each with a specific error message:

- `--color` conflicts with `--rgb` (clap `conflicts_with`).
- `--rgb` / `--color` are rejected on `cct:N` and `light:N` components.
- `--temp` is only valid on `cct:N`.
- `--white` is only valid on `rgbw:N`.
- `--brightness` is valid on all four component kinds.

## Capability detection

Detected **live at command execution** via a single `Shelly.GetStatus` call, not
from the device cache. Rationale: avoids a cache-schema migration, and is never
stale. Implementation scans the status object keys for prefixes `rgb:`, `rgbw:`,
`cct:`, `light:` and records `{ kind, id }` for each. This set drives:

- `--id` range validation (analogous to `count_gen2_outputs` in `src/api/mod.rs`).
- Which flags are legal for the targeted component.

## RPC layer (`src/api/gen2.rs`)

Add to `Gen2Device`:

- `light_components() -> Result<Vec<LightComponent>>` - parses `Shelly.GetStatus`.
- `light_set(kind, id, params) -> Result<...>` - routes to `RGB.Set` / `RGBW.Set`
  / `CCT.Set` / `Light.Set`, building the JSON body for that kind.
- `light_status(kind, id) -> Result<...>` - reads current state for `status`.

The Gen1 path (`src/api/gen1.rs` via the `ShellyDevice` enum) returns the
"not yet supported" error for all light operations.

### Confirmed RPC ranges (from official Shelly Gen2 docs, verified 2026-05-24)

| Method | `rgb` | `white` | `brightness` | other | required |
|--------|-------|---------|--------------|-------|----------|
| `RGB.Set`   | `[r,g,b]` each `0..255` | - | `1..100` | - | at least one of `on`/`brightness` |
| `RGBW.Set`  | `[r,g,b]` each `0..255` | `0..255` | `1..100` | - | at least one of `on`/`brightness` |
| `CCT.Set`   | - | - | `0..100` | `ct` Kelvin (device range) | at least one of `on`/`brightness`/`ct` |
| `Light.Set` | - | - | `0..100` | - | at least one of `on`/`brightness` |

### Example RPC bodies

```
RGB.Set   { "id": 0, "on": true, "rgb": [0,255,136], "brightness": 80 }
RGBW.Set  { "id": 0, "on": true, "rgb": [0,255,136], "white": 0, "brightness": 80 }
CCT.Set   { "id": 0, "on": true, "ct": 3000, "brightness": 80 }
Light.Set { "id": 0, "on": true, "brightness": 80 }
```

Omitted attributes are left unchanged by the device. `on` sends `on: true`; `set`
sends `on: <current state>` (from the detection `GetStatus`) so it both preserves
power and satisfies the "at least one of `on`/`brightness`" requirement above.

## Error handling

- Device has no light component:
  `"<name> has no RGB/light outputs (model <model>). 'light' supports Gen2/Gen3 RGB, RGBW, CCT, and dimmable devices."`
- Wrong flag for component kind: specific message naming the flag and the component
  kind (e.g. `"--temp is only valid for color-temperature (cct) lights; <name> has an rgb output"`).
- `--id` out of range: mirrors `validate_switch_id` wording, listing valid IDs.
- Gen1 device: `"light control for Gen1 devices is not yet implemented (planned)"`.

## Testing

- **Color parsing (no hardware):** exhaustive unit tests - valid/invalid hex, every
  named color, rgb triple bounds (0/255/256/negative/malformed), `--color`/`--rgb`
  conflict.
- **Component detection (no hardware):** unit tests over sample `Shelly.GetStatus`
  JSON containing `rgb:0`, `rgbw:0`, `cct:0`, `light:0`, and mixtures; assert correct
  `{kind,id}` sets and `--id` validation.
- **RPC body construction (no hardware):** assert each Set payload matches the
  documented shape and ranges for representative flag combinations, including `set`
  carrying `on: <current>` and per-kind brightness bounds (1-100 rgb/rgbw, 0-100
  cct/light).
- **Live hardware constraint (honest):** no Gen2/Gen3 RGB device is available in the
  local cache (the Gen3 Mini 1PM is a switch). The `*.Set` round-trip therefore
  cannot be exercised against real hardware in this environment. Logic is covered by
  unit tests; live verification is deferred until RGB hardware is available, and the
  issue reporter (@bricelb) may help validate.

## Open questions

RPC parameter ranges are resolved (see Confirmed RPC Ranges). One minor decision
remains, safe to settle in implementation:

- Final named-color palette. Proposed: `red, green, blue, white, warm, cyan, magenta,
  yellow, orange, purple, pink, off` mapping to fixed RGB values, extensible later.

## Affected / new files

- `src/color.rs` (new) - color parsing module.
- `src/cli/mod.rs` - `Light` command + `LightAction` subcommands and flags.
- `src/main.rs` - `cmd_light` handler, `--id` validation reuse.
- `src/api/gen2.rs` - `light_components`, `light_set`, `light_status`.
- `src/api/mod.rs` / `src/api/gen1.rs` - dispatch + Gen1 "not supported" path.
- `src/output.rs` - `light status` rendering.
- `src/schema.rs` - register `light on`, `light off`, `light toggle`, and `light set`
  in the hardcoded `mutating_commands` list (`src/schema.rs:14`), matching how the
  `switch` actions are marked. Without this, `shelly schema` advertises the new
  state-changing commands as read-only, breaking the agent-integration contract.
  `light status` stays non-mutating.
- Tests colocated per existing convention. Add a `schema` test asserting the new
  light mutating commands are reported with `is_mutating: true`.
