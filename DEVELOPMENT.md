# Development Notes

## Overlay Responsibilities

The app has three overlay responsibilities with separate lifecycles:

- Status overlay: the persistent minimal indicator for current runtime state. It reads snapshots such as active/idle, drag, slow movement, jump/final-adjust state, and speed flashes without owning the behavior that produced them.
- Tooltip/help overlay: temporary explanatory text plus the Slash help panel. Temporary notices are event-driven and expire; the Slash panel is an explicit help view with runtime stats and keybinds that stays visible until toggled or dismissed.
- Jump overlay: the jump UI. It owns the staged grid, labels, selected/preview regions, and final crosshair visuals while jump mode is active.

Input event precedence should remain explicit: emergency/system handling and dismissals run before regular action routing, exclusive modes such as jump consume their own input, and normal action bindings run only after those higher-priority paths have had a chance to handle the event.

State mutation and UI rendering are deliberately decoupled. Input handlers update runtime state and emit commands or notifications; overlay renderers consume snapshots of that state on their own update path. This keeps key routing deterministic, prevents painting code from deciding behavior, and lets overlays be hidden, throttled, or disabled without changing the command semantics.

## Jump Config Migration

The current jump schema is rooted at `[jump]` and uses:

```toml
[jump]
mode = "precision"
cursor_between_stages = "none"
start_region = "virtual_screen"
preview_edge_behavior = "clamp"
```

The stage tables are `[jump.coarse]`, `[jump.fine]`, and `[jump.precise]`.
Each stage owns geometry fields (`enabled`, `width`, `height`, `aim_point`,
`aim_offset_x_px`, `aim_offset_y_px`, `target_region_mode`,
`target_margin_percent`, `visual_context_margin_percent`, `zoom_scale`,
and optional `preview_edge_behavior`) plus a nested `.labels` table
(`font_scale`, `center_marker`, `separators`, `hide_threshold_px`).

Deprecated compatibility behavior:

- `jump.move_cursor_after_each_stage = true` maps to
  `jump.cursor_between_stages = "move_to_region_center"` when the modern field
  is absent.
- `jump.move_cursor_after_each_stage = false` maps to
  `jump.cursor_between_stages = "none"` when the modern field is absent.
- If both `cursor_between_stages` and `move_cursor_after_each_stage` are present,
  `cursor_between_stages` wins.
- Stage-level `preview_margin_percent` maps to
  `visual_context_margin_percent` only when `visual_context_margin_percent` is
  absent.
- Invalid `target_region_mode` strings intentionally fall back to
  `exact_region` during normalization with a config warning.
- Other enum-backed strings, such as `cursor_between_stages`, `start_region`,
  `preview_edge_behavior`, and `aim_point`, are serde-parsed enums and should
  fail configuration loading with a clear unknown-variant error.

Compatibility and deprecation table:

| Old field or value | New field or value | Fallback behavior | Planned removal policy |
| --- | --- | --- | --- |
| `jump.move_cursor_after_each_stage = true` | `jump.cursor_between_stages = "move_to_region_center"` | Used only when `cursor_between_stages` is absent. | Keep through the current compatibility window; remove after checked-in config, README, and smoke-test docs no longer mention the old field except in migration notes. |
| `jump.move_cursor_after_each_stage = false` | `jump.cursor_between_stages = "none"` | Used only when `cursor_between_stages` is absent. | Same policy as the `true` mapping. |
| Both `jump.move_cursor_after_each_stage` and `jump.cursor_between_stages` | `jump.cursor_between_stages` | Modern field wins; legacy field is ignored. | Keep precedence until the legacy field is removed, then unknown-field handling can reject it if strict parsing is enabled. |
| `jump.<stage>.preview_margin_percent` | `jump.<stage>.visual_context_margin_percent` | Legacy field is copied only when `visual_context_margin_percent` is absent. | Remove after one release or migration cycle with warning coverage in tests. |
| Invalid `jump.<stage>.target_region_mode` string | Valid `target_region_mode`: `exact_region`, `region_with_context`, `expanded_target`, or `cursor_centered_zoom` | Normalizes to `exact_region` and records a config warning. | Keep fallback while users may have experimental values; revisit when config validation becomes stricter. |
| Missing `[jump.coarse]` with `grid_size = { width, height }` | `[jump.coarse].width` and `[jump.coarse].height` | Coarse jump size is derived from `grid_size`; normalized config writes the coarse size back into runtime state. | Keep until `grid_size` is no longer documented as a legacy fallback. |
| `starting_speed` | `[mouse_speed].default_speed` | Seeds `mouse_speed.default_speed` only when `[mouse_speed]` is otherwise at its default. Runtime `starting_speed` is normalized to the effective mouse default. | Keep as a legacy migration path; prefer removing after configs have moved fully to `[mouse_speed]`. |
| `middle_mouse` action | `middle_click` | Action parser accepts both names. | Alias can remain indefinitely because it is harmless and isolated. |
| `scroll_up`, `scroll_down`, `scroll_left`, `scroll_right` actions | `wheel_up`, `wheel_down`, `wheel_left`, `wheel_right` | Action parser accepts both names. | Alias can remain indefinitely unless action help output becomes canonical-only. |
| `drag_mode`, `toggle_left_drag`, `toggle_left_button_hold` actions | `toggle_drag_mode` | Action parser accepts all aliases. | Alias can remain indefinitely because existing user bindings are low risk. |
| `center_monitor` action | `center_current_monitor` | Action parser accepts both names. | Alias can remain indefinitely. |
| `movement_profile.*`, `mouse_profile:*`, `mouse_profile.*`, `select_movement_profile:*` | `movement_profile:<name>` | Action parser accepts all forms and selects the named movement profile. | Keep aliases while profile binding syntax is user-facing; document only canonical `movement_profile:<name>` and `movement_profile.<name>`. |
| `wheel_profile.*`, `select_wheel_profile:*` | `wheel_profile:<name>` | Action parser accepts all forms and selects the named wheel profile. | Keep aliases while profile binding syntax is user-facing; document only canonical `wheel_profile:<name>` and `wheel_profile.<name>`. |

Migration notes:

1. Replace `move_cursor_after_each_stage = true` with
   `cursor_between_stages = "move_to_region_center"`.
2. Replace `move_cursor_after_each_stage = false` with
   `cursor_between_stages = "none"`.
3. Add `start_region` and `preview_edge_behavior` to `[jump]` so defaults are
   explicit in checked-in configs.
4. Add `target_region_mode`, `target_margin_percent`,
   `visual_context_margin_percent`, and `zoom_scale` to each stage table.
5. Move stage label settings into `[jump.coarse.labels]`,
   `[jump.fine.labels]`, and `[jump.precise.labels]`.
6. Prefer `visual_context_margin_percent`; keep `preview_margin_percent` only
   for reading older user configs.

## System Binding Migration

`config.toml` currently loads all `key_bindings` through the same key-action path in
`Config::initialize_bindings`, which means `["Escape", "exit"]` is a valid configured
binding and reaches `AppState::route_key_event` as `Some(Action::Exit)`.

`AppState` also preserves a hardcoded Escape key-down exit as an emergency fallback.
That branch runs before generic action routing and returns immediately, so if Escape
is both hardcoded and configured it still emits exactly one `AppCommand::Exit` for a
single key-down.

The intended short-term model is:

```toml
[system_bindings]
toggle_active = "Alt+E"
exit = "Escape"
```

Keep these controls separate from movement/action bindings because they must remain
available when active mode is disabled. Migration steps:

1. Add a `system_bindings` config section with defaults matching the current
   hardcoded controls.
2. Parse system chords separately from `key_bindings`, including modifier-aware
   matches for entries such as `Alt+E`.
3. Route system bindings before movement/action bindings and before the active-mode
   gate.
4. Keep Escape hardcoded while the new section is optional, and log or test that the
   configured exit binding and hardcoded fallback do not double-emit.
5. After the system binding section is stable and covered by tests, remove the
   hardcoded Escape branch and require the default `system_bindings.exit` value to
   provide the emergency exit.

## Post-Core Backlog: Precision Follow-Ups

These items are **post-core** and are blocked until the core precision fix and its regression test coverage are complete.

### 1) Optional slow-entry tooltip event

- Add `RuntimeNotificationKind::SlowMouse` for slow-mode entry/exit notifications.
- Add `tooltip_overlay.events.slow` configuration gating for this event category.
- Emit the notification only on the inactive -> active slow-mode transition (not on every tick while held).

### 2) Optional independent slow acceleration runtime state

- Split slow-mode counters/state from normal movement runtime state.
- Do not reuse normal acceleration fields for slow-mode acceleration behavior.
- Keep the separation explicit in runtime structures so future tuning does not couple slow-mode and normal-mode internals.

### 3) Optional new slow-mode actions

- Add actions: `slow_speed_up`, `slow_speed_down`, `slow_speed_reset`, `toggle_slow_mouse`.
- Include binding examples in docs/config samples.
- Avoid enabling crowded defaults by default; keep default keymap conservative.

### 4) Default keymap ergonomics follow-up

- Add a default `mouse_speed_up` binding counterpart to existing default down/reset bindings.
- Add a commented example showing direct binding to a precision movement profile.

### Dependency gate

- Do not start any of the above until the core precision fix is landed and regression tests pass.
