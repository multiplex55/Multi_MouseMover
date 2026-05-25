# Multi MouseMover

Multi MouseMover is a Windows keyboard-to-mouse utility written in Rust. It installs a global keyboard hook, routes configured key chords into mouse actions, and shows lightweight overlays for status, jump grids, help, and optional final cursor adjustment.

## Quick Start

1. Install [Rust](https://www.rust-lang.org/tools/install) and make sure `cargo` is available in your `PATH`.
2. Build the release binary:

```bash
cargo build --release
```

3. Run it from the project root so the checked-in `config.toml` is found:

```bash
cargo run --release
```

Administrator privileges are often needed for Windows global hooks. Configuration lives in `config.toml`; edit it, then press `RightAlt+R` to reload without restarting. If reload validation fails, the old runtime config stays active.

The crate and binary id is `multi_mousemover`. On Windows, `cargo build` produces `target/debug/multi_mousemover.exe`, and `cargo build --release` produces `target/release/multi_mousemover.exe`.

## Default Bindings

System bindings are always available, even while the app is idle:

| Key | Action |
| --- | --- |
| `Ctrl+E` / `Ctrl+Q` | Toggle active/idle mode (depends on checked-in config) |
| `Escape` | Exit |

Action bindings run only while active mode is enabled:

| Key | Action |
| --- | --- |
| `W` | Move up |
| `A` | Move left |
| `S` | Move down |
| `D` | Move right |
| `LeftShift` | Slow held movement |
| `SPACE` | Left click |
| `L` | Right click |
| `RightShift` | Middle click |
| `N` | Toggle left-button drag mode |
| `.` | Click, then switch to idle |
| `,` / `M` | Wheel up / down |
| `I` / `O` | Wheel left / right |
| `X` / `Z` | Mouse speed down / reset |
| `V` / `B` | Wheel speed up / down |
| `RightAlt+C` / `RightAlt+X` | Next / previous movement profile |
| `RightAlt+V` / `RightAlt+B` | Next / previous wheel profile |
| `F` | Jump mode |
| `G` | Grid mode |
| `C` | Screen select |
| `H` | Navigate back |
| `Y` | Navigate forward |
| `Q` / `P` | Switch to idle |
| `RightAlt+W` / `RightAlt+A` / `RightAlt+S` / `RightAlt+D` | Move cursor to screen edge |
| `Alt+E` / `Alt+S` / `Alt+D` / `Alt+F` | Step move up / left / down / right (normal tier) |
| `Alt+Ctrl+E` / `Alt+Ctrl+S` / `Alt+Ctrl+D` / `Alt+Ctrl+F` | Step move up / left / down / right (small tier) |
| `Alt+Shift+E` / `Alt+Shift+S` / `Alt+Shift+D` / `Alt+Shift+F` | Step move up / left / down / right (large tier) |
| `RightAlt+R` | Reload `config.toml` |
| `RightAlt+Escape` | Panic reset |
| `/` | Toggle help tooltip/panel with runtime stats + keybinds |

## Active And Idle

The app starts active. Press `Ctrl+E` (or `Ctrl+Q` in configs that set that as `system_bindings.toggle_active`) to switch between active and idle mode. In active mode, configured action bindings are swallowed and translated into mouse commands. In idle mode, normal action bindings are ignored so the same keys can pass through to Windows and other apps.

System bindings bypass the active-mode gate. The configured toggle-active chord (commonly `Ctrl+E` or `Ctrl+Q`) can always reactivate control, and `Escape` remains the configured exit key.

## Drag Behavior

Press `N` to toggle left-button drag mode. When drag is on, Multi MouseMover holds the left mouse button down so movement keys can drag windows, text selections, sliders, or canvas objects. Press `N` again to release it.

`RightAlt+Escape` performs a panic reset: it releases drag, clears currently held actions, exits jump mode, hides overlays, resets runtime speed tiers, and returns to active mode.

## Wheel And Speed Controls

Hold `,`, `M`, `I`, or `O` for repeated vertical or horizontal wheel ticks. Wheel repeat timing comes from `[wheel].tick_interval`; wheel strength comes from the current wheel speed and axis multipliers.

Mouse movement has two layers. `[mouse_speed]` controls the baseline tier changed by `X` and `Z`; `acceleration`, `acceleration_rate`, and `top_speed` then shape how held movement ramps while a direction key is down. `LeftShift` slows movement while held.

Profiles let you swap groups of speed settings at runtime:

```toml
["RightAlt+C", "movement_profile_next"]
["RightAlt+X", "movement_profile_previous"]
["RightAlt+V", "wheel_profile_next"]
["RightAlt+B", "wheel_profile_previous"]
```

Runtime action syntax also supports direct profile selection with either `movement_profile:<name>` / `movement_profile.<name>` and `wheel_profile:<name>` / `wheel_profile.<name>`.

## Jump Stages And Profiles

Press `F` to enter jump mode. A labeled grid appears over the start region. Type the visible cell label to narrow the target or complete the jump. The default config uses precision mode with `coarse`, `fine`, and `precise` stages.

The base `[jump]` table chooses the mode, start region, cursor behavior between stages, preview edge behavior, and visuals. Each stage table controls grid size, target region behavior, zoom, context margin, aim point, and labels.

Jump profiles live under `jump.profiles.<name>` and merge over the base jump settings for one launch. Profiles can override just the fields they need:

```toml
["F", "jump_mode"]
["RightAlt+F", "jump_mode_profile:precise"]
["RightAlt+W", "jump_mode_profile.window"]
```

The default config includes `fast`, `precise`, `window`, and `monitor` profiles. `fast` is a single-stage jump. `precise` keeps multiple stages and previews between them. `window` starts from the active window bounds. `monitor` starts from the current monitor.

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


## UI Hints Mode

UI Hints mode discovers accessible controls from the foreground app and overlays short labels so you can jump the cursor directly to UI targets. Example binding in `config.toml`:

```toml
["0", "ui_hint_mode"]
```

Interaction flow:

1. Press the UI Hints binding.
2. Labels appear after the UI Automation (UIA) query completes.
3. Type the label for your target control.
4. `Escape` cancels and exits UI Hints mode at any point.

Optional tooltip events for query lifecycle can be toggled under `[tooltip_overlay.events]`:

- `ui_hints_query_start`
- `ui_hints_query_fail`
- `ui_hints_query_empty`
- `ui_hints_query_capped_count`

Limitations and caveats:

- Some apps expose weak or incomplete UIA trees, so hint coverage can be sparse or inconsistent.
- Elevation boundaries apply: if the foreground app is elevated and Multi MouseMover is not (or vice versa), UIA visibility and interaction can be reduced.
- Custom-rendered or game UIs often provide little/no actionable UIA metadata, so hints may be missing.
- High-density UI apps (especially browsers/Electron apps) can generate crowded overlays; tune `ui_hints.min_hint_spacing_px` and `ui_hints.max_hints` to reduce noise.

## Final Adjust

`[final_adjust]` is disabled by default. When enabled, a completed jump enters a small adjustment state before the cursor move is committed. Use the movement keys to nudge by `small_step_px`; hold `Shift` for `large_step_px`; press `Enter` to confirm, `Escape` to cancel, or `Backspace` to return to the previous jump stage. The status overlay can show when final-adjust is active.

## Overlays

`[status_overlay]` controls the compact persistent status indicator. It can show active/idle, drag, slow movement, jump state, speed flashes, wheel state, and final-adjust state.

`[tooltip_overlay]` controls temporary explanatory messages and the larger help panel toggled by `/`. Temporary tooltips are short-lived notices for runtime changes such as mouse speed, wheel speed, profile changes, drag, reload, and panic reset. The help panel does not auto-expire; it shows current runtime stats plus configured keybinds until `/` toggles it again, Escape dismisses it, or the app enters an exclusive mode such as jump.

Use `enabled = false` to disable all tooltip/help overlay rendering, `show_temporary_tooltips = false` to keep `/` help while hiding short runtime notices, and `show_help = false` to keep runtime notices while disabling the help panel. Individual temporary trigger classes can be controlled under `[tooltip_overlay.events]`:

```toml
[tooltip_overlay]
enabled = true
show_temporary_tooltips = true
show_help = true
positioning = "cursor"
duration_ms = 900
help_positioning = "center"
help_width = 420
help_max_bindings = 40

[tooltip_overlay.events]
mouse = true
wheel = true
profile = true
drag = true
reload = true
panic = true
```

The tooltip/help overlay is intentionally separate from `[status_overlay]`: status is a small always-on indicator for current state, while tooltip/help is temporary explanatory text or the explicit Slash panel.

Jump overlays are configured under `[jump.visuals]` and per-stage label tables. You can show or hide selected-region outlines, preview outlines, active-grid outlines, cell centers, and the final crosshair.

## Safety

`Escape` exits through `[system_bindings]`. `RightAlt+Escape` is the runtime panic reset and is useful if drag is stuck, a jump overlay is active, or held keys need to be cleared.

`RightAlt+R` reloads `config.toml` atomically from the user perspective: invalid configs are rejected and the last valid runtime config remains active. Compatibility fields are still accepted for older configs, but new configs should use the current field names documented in [DEVELOPMENT.md](DEVELOPMENT.md).

## Troubleshooting

For input-pipeline regressions, run the diagnostic smoke test in [docs/smoke_test_input_pipeline.md](docs/smoke_test_input_pipeline.md). It uses the current `MULTI_MOUSEMOVER_DEBUG=1` flag and lists the expected startup, heartbeat, key routing, command, click, and exit log fragments.

## License

This project is licensed under the terms of the MIT license. See [LICENSE](LICENSE) for details.
