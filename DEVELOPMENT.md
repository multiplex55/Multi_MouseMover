# Development Notes

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
