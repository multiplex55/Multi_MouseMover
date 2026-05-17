# Multi MouseMover

Multi MouseMover is a Windows utility written in Rust that lets you control the mouse entirely from the keyboard.  It installs a global keyboard hook and translates key presses into mouse movement and clicks.  A small overlay shows the current click state and a jump mode lets you instantly reposition the cursor using an on–screen grid.

## Setup

1. Install [Rust](https://www.rust-lang.org/tools/install) and ensure `cargo` is in your `PATH`.
2. Build the project:

```bash
cargo build --release
```

3. Run the compiled binary (administrator privileges are usually required for global hooks):

```bash
cargo run --release
```

## Artifact names

The crate and binary id is `multi_mousemover`. On Windows, `cargo build` produces
`target/debug/multi_mousemover.exe`, and `cargo build --release` produces
`target/release/multi_mousemover.exe`.

Configuration lives in `config.toml` in the project root. Key bindings, mouse parameters, and jump-mode geometry can be tweaked there.

## Keybindings

Default bindings are defined in `config.toml` under the `key_bindings` table.  Some important actions:

| Key          | Action          |
|--------------|-----------------|
| `W`          | Move up         |
| `A`          | Move left       |
| `S`          | Move down       |
| `D`          | Move right      |
| `Space`      | Left click      |
| `L`          | Right click     |
| `LeftShift`/`RightShift` | Slow movement |
| `Escape`     | Exit the program|
| `F`          | Enter jump mode |
| `RightAlt+R` | Reload config |
| `RightAlt+Escape` | Panic reset |

Holding **Alt + E** toggles between *Active* and *Idle* modes where keybinds are processed or ignored respectively.

## Jump Mode

Press the `F` key to activate *jump mode*. A translucent grid appears over the screen labelled with letter pairs. Type the displayed sequence (for example `AA`, `AB`, etc.) to move the cursor to that grid cell. In `precision` mode, jump can advance through coarse, fine, and precise stages before the final cursor move.

Example sequence:

1. Hit `F` – the grid overlay appears.
2. Enter the letter pair shown in the target cell.
3. The mouse jumps to that position and the overlay hides.

Jump profiles live under `jump.profiles.<name>` and override the base `[jump]`
settings only for that launch. Bind one with either action syntax:

```toml
["F", "jump_mode"]
["RightAlt+F", "jump_mode_profile:precise"]
["RightAlt+W", "jump_mode_profile.window"]
```

The default `config.toml` includes practical `fast`, `precise`, `window`, and
`monitor` jump profiles. Profile tables merge over the base jump config, so a
profile can set only `start_region`, `mode`, or one stage without repeating the
entire jump block.

## Configuration Options

`config.toml` exposes several tunables:

- `key_bindings` – mapping of keyboard keys to actions.
- `system_bindings` – always-available controls such as active-mode toggle and exit.
- `polling_rate` – delay (ms) between input polls.
- `grid_size` – legacy width/height for the coarse jump grid when `jump.coarse` is omitted.
- `jump.mode` – `single` or `precision`.
- `jump.cursor_between_stages` – `none`, `move_to_region_center`, `preview_only`, or `warp_and_continue`.
- `jump.start_region` – `virtual_screen`, `current_monitor`, `active_window_monitor`, or `active_window_bounds`.
- `jump.preview_edge_behavior` – `clamp`, `shift_into_bounds`, `allow_asymmetric_context`, or `disable_context_near_edges`.
- `jump.visuals` – toggles selected-region outline, preview outline, active-grid outline, cell centers, and final crosshair.
- `jump.coarse`, `jump.fine`, `jump.precise` – per-stage geometry and label settings.
- `jump.profiles.<name>` – named jump overrides selected with `jump_mode_profile:<name>`.
- `starting_speed` – initial mouse speed in pixels per step.
- `acceleration` and `acceleration_rate` – how quickly speed increases when holding a direction.
- `top_speed` – maximum mouse speed.
- `movement_profiles.<name>` – named mouse speed profiles selected with `movement_profile:<name>`, `movement_profile_next`, or `movement_profile_previous`.
- `wheel` – wheel speed, bounds, tick interval, indicator duration, and vertical/horizontal axis multipliers.
- `wheel_profiles.<name>` – named wheel overrides selected with `wheel_profile:<name>`, `wheel_profile_next`, or `wheel_profile_previous`.
- `edge_jump` – edge-jump offset and work-area behavior.

Runtime action syntax:

- Mouse speed: `mouse_speed_up`, `mouse_speed_down`, `mouse_speed_reset`.
- Movement profiles: `movement_profile_next`, `movement_profile_previous`, `movement_profile:<name>`, or `movement_profile.<name>`.
- Wheel speed: `wheel_speed_up`, `wheel_speed_down`, `wheel_speed_reset`.
- Wheel profiles: `wheel_profile_next`, `wheel_profile_previous`, `wheel_profile:<name>`, or `wheel_profile.<name>`.
- Config/runtime safety: `reload_config` reloads `config.toml` and keeps the old config on validation failure; `panic_reset` releases drag, clears active keys, exits jump mode, hides overlays, and resets runtime speeds.

Each jump stage supports:

- `enabled`, `width`, `height`
- `aim_point` – `center`, `top_left`, `top_right`, `bottom_left`, `bottom_right`, or `custom_offset`
- `aim_offset_x_px`, `aim_offset_y_px` for `custom_offset`
- `target_region_mode` – `exact_region`, `region_with_context`, `expanded_target`, or `cursor_centered_zoom`
- `target_margin_percent`, `visual_context_margin_percent`, `zoom_scale`
- `preview_edge_behavior` as an optional per-stage override
- nested `labels` with `font_scale`, `center_marker`, `separators`, and `hide_threshold_px`

Copy-pasteable jump configuration:

```toml
[jump]
mode = "precision"
cursor_between_stages = "none"
start_region = "virtual_screen"
preview_edge_behavior = "clamp"

[jump.visuals]
selected_region_outline = true
preview_outline = true
active_grid_outline = true
cell_centers = false
final_crosshair = true

[jump.coarse]
enabled = true
width = 10
height = 10
aim_point = "center"
aim_offset_x_px = 0
aim_offset_y_px = 0
target_region_mode = "exact_region"
target_margin_percent = 0
visual_context_margin_percent = 0
zoom_scale = 1.0

[jump.coarse.labels]
font_scale = 1.0
center_marker = false
separators = true
hide_threshold_px = 0

[jump.fine]
enabled = true
width = 8
height = 8
aim_point = "center"
aim_offset_x_px = 0
aim_offset_y_px = 0
target_region_mode = "exact_region"
target_margin_percent = 0
visual_context_margin_percent = 1
zoom_scale = 1.5

[jump.fine.labels]
font_scale = 1.0
center_marker = false
separators = true
hide_threshold_px = 0

[jump.precise]
enabled = true
width = 5
height = 5
aim_point = "center"
aim_offset_x_px = 0
aim_offset_y_px = 0
target_region_mode = "exact_region"
target_margin_percent = 0
visual_context_margin_percent = 5
zoom_scale = 2.5

[jump.precise.labels]
font_scale = 1.0
center_marker = false
separators = true
hide_threshold_px = 0
```

Deprecated compatibility fields are still accepted: `jump.move_cursor_after_each_stage = true` maps to `cursor_between_stages = "move_to_region_center"`, and `false` maps to `"none"` when `cursor_between_stages` is absent. Stage-level `preview_margin_percent` maps to `visual_context_margin_percent` only when the modern field is absent.

Adjust these values to suit your workflow. After editing the file, use the
`reload_config` action or restart the application to apply changes.

## Troubleshooting

For input-pipeline regressions, run the diagnostic smoke test in
[docs/smoke_test_input_pipeline.md](docs/smoke_test_input_pipeline.md). It uses the
current `MULTI_MOUSEMOVER_DEBUG=1` flag and lists the expected startup, heartbeat,
key routing, command, click, and exit log fragments.

## License

This project is licensed under the terms of the MIT license.  See [LICENSE](LICENSE) for details.
