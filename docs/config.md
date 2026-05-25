# Configuration Reference

This is the canonical reference for `config.toml`. Paths marked Active are read by the current schema. Legacy paths are still read for compatibility. Deprecated alias paths are accepted only as migration shims and should be replaced in edited configs. Preview-related paths affect jump preview rendering or experimental preview behavior.

Status values used here: Active, Legacy, Deprecated alias, Preview-related.

## Key Binding Example (Wheel + Wheel Speed)

Example layout using vertical wheel + wheel-speed controls (with horizontal wheel bindings intentionally disabled):

```toml
key_bindings = [
  ["W", "wheel_up"],
  ["R", "wheel_down"],
  # ["I", "wheel_left"],
  # ["O", "wheel_right"],
  ["U", "wheel_speed_up"],
  ["I", "wheel_speed_down"],
]
```

### Scroll mode behavior mapping

When `scroll_mode.enabled = true` and the configured `scroll_mode.modifier_action` is held, movement actions are remapped to wheel actions for that tick:

- `move_up` -> `wheel_up` (vertical wheel negative delta; typical content moves up).
- `move_down` -> `wheel_down` (vertical wheel positive delta; typical content moves down).
- `move_left` -> `wheel_left` (horizontal wheel negative delta; typical content moves left).
- `move_right` -> `wheel_right` (horizontal wheel positive delta; typical content moves right).

Example with held modifier (`LeftAlt` bound to `scroll_modifier`):

```toml
key_bindings = [
  ["E", "move_up"],
  ["S", "move_left"],
  ["D", "move_down"],
  ["F", "move_right"],
  ["LeftAlt", "scroll_modifier"],
]

[scroll_mode]
enabled = true
modifier_action = "scroll_modifier"
```

Holding `LeftAlt+E` emits `wheel_up` instead of moving the cursor; releasing `LeftAlt` restores normal movement.

### Default binding list and edge-jump distinction

The checked-in `config.toml` defaults to:

- Monitor-edge actions:
  - `RightAlt+W` -> `move_to_top_edge`
  - `RightAlt+A` -> `move_to_left_edge`
  - `RightAlt+S` -> `move_to_bottom_edge`
  - `RightAlt+D` -> `move_to_right_edge`
- Active-window-edge actions:
  - `RightAlt+Ctrl+W` -> `move_to_window_top_edge`
  - `RightAlt+Ctrl+A` -> `move_to_window_left_edge`
  - `RightAlt+Ctrl+S` -> `move_to_window_bottom_edge`
  - `RightAlt+Ctrl+D` -> `move_to_window_right_edge`
  - `RightAlt+Ctrl+Q` -> `move_to_window_center`
  - `RightAlt+Ctrl+E` -> `move_to_window_titlebar`

Full default `key_bindings` list:

```toml
key_bindings = [
  ["W", "move_up"], ["A", "move_left"], ["S", "move_down"], ["D", "move_right"],
  ["LeftShift", "slow_mouse"],
  ["SPACE", "left_click"], ["L", "right_click"], ["RightShift", "middle_click"],
  ["N", "toggle_drag_mode"], [".", "click_then_disable"],
  ["W", "wheel_up"], ["R", "wheel_down"],
  ["X", "mouse_speed_down"], ["Z", "mouse_speed_reset"],
  ["U", "wheel_speed_up"], ["I", "wheel_speed_down"],
  ["RightAlt+C", "movement_profile_next"], ["RightAlt+X", "movement_profile_previous"],
  ["RightAlt+V", "wheel_profile_next"], ["RightAlt+B", "wheel_profile_previous"],
  ["F", "jump_mode"], ["G", "grid_mode"], ["C", "screen_select"],
  ["H", "navigate_back"], ["Y", "navigate_forward"],
  ["Q", "disable"], ["P", "disable"],
  ["RightAlt+W", "move_to_top_edge"], ["RightAlt+A", "move_to_left_edge"],
  ["RightAlt+S", "move_to_bottom_edge"], ["RightAlt+D", "move_to_right_edge"],
  ["RightAlt+Ctrl+W", "move_to_window_top_edge"], ["RightAlt+Ctrl+A", "move_to_window_left_edge"],
  ["RightAlt+Ctrl+S", "move_to_window_bottom_edge"], ["RightAlt+Ctrl+D", "move_to_window_right_edge"],
  ["RightAlt+Ctrl+Q", "move_to_window_center"], ["RightAlt+Ctrl+E", "move_to_window_titlebar"],
  ["RightAlt+R", "reload_config"], ["RightAlt+Escape", "panic_reset"],
  ["Alt+E", "step_move_up"], ["Alt+S", "step_move_left"], ["Alt+D", "step_move_down"], ["Alt+F", "step_move_right"],
  ["Alt+Ctrl+E", "step_move_small_up"], ["Alt+Ctrl+S", "step_move_small_left"], ["Alt+Ctrl+D", "step_move_small_down"], ["Alt+Ctrl+F", "step_move_small_right"],
  ["Alt+Shift+E", "step_move_large_up"], ["Alt+Shift+S", "step_move_large_left"], ["Alt+Shift+D", "step_move_large_down"], ["Alt+Shift+F", "step_move_large_right"],
  ["/", "show_help"]
]
```

## Config Paths

| Config path | Type | Default | Connected? | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| `polling_rate` | integer milliseconds | `8` | Yes | Active | Main input loop delay. Lower values poll more often. |
| `key_bindings` | array of `[key, action]` pairs | built-in movement/click/jump bindings | Yes | Active | Active-mode action bindings. |
| `system_bindings.toggle_active` | key chord string | `"Ctrl+E"` | Yes | Active | Global active/idle toggle. |
| `system_bindings.exit` | key chord string | `"Escape"` | Yes | Active | Global exit binding. |
| `mouse_speed.default_speed` | integer | `1` | Yes | Active | Normal held-movement speed and replacement for legacy `starting_speed`. |
| `mouse_speed.min_speed` | integer | `1` | Yes | Active | Lower bound for runtime mouse speed changes. |
| `mouse_speed.max_speed` | integer | `12` | Yes | Active | Upper bound for runtime mouse speed changes. |
| `mouse_speed.speed_step` | integer | `1` | Yes | Active | Increment used by mouse speed up/down actions. |
| `mouse_speed.flash_indicator_ms` | integer milliseconds | `700` | Yes | Active | Duration for mouse-speed feedback. |
| `slow_mouse.strategy` | enum string | `"fixed"` | Yes | Active | Slow-mode speed strategy: `fixed`, `multiplier`, or `subtract`. |
| `slow_mouse.fixed_speed` | integer | `1` | Yes | Active | Target speed used when `strategy = "fixed"`. |
| `slow_mouse.multiplier` | float | `0.25` | Yes | Active | Multiplies current normal speed when `strategy = "multiplier"`. |
| `slow_mouse.subtract_speed` | integer | `4` | Yes | Active | Amount subtracted from current normal speed when `strategy = "subtract"`. |
| `slow_mouse.min_speed` | integer | `1` | Yes | Active | Lower clamp applied after strategy calculation. |
| `slow_mouse.max_speed` | integer | `2` | Yes | Active | Upper clamp applied after strategy calculation. |
| `slow_mouse.acceleration` | integer | `0` | Yes | Active | Slow-mode ramp increment applied while held movement continues. |
| `slow_mouse.acceleration_rate` | integer polling cycles | `1` | Yes | Active | Slow-mode ramp cadence in polling cycles. |
| `movement_profiles.*.default_speed` | integer | none | Yes | Active | Optional named movement profile override. |
| `movement_profiles.*.min_speed` | integer | none | Yes | Active | Optional named movement profile override. |
| `movement_profiles.*.max_speed` | integer | none | Yes | Active | Optional named movement profile override. |
| `movement_profiles.*.speed_step` | integer | none | Yes | Active | Optional named movement profile override. |
| `movement_profiles.*.flash_indicator_ms` | integer milliseconds | none | Yes | Active | Optional named movement profile override. |
| `starting_speed` | integer | `1` | Yes | Legacy | Compatibility alias used to seed `mouse_speed.default_speed` only when `[mouse_speed]` is otherwise default. |
| `acceleration` | integer | `2` | Yes | Legacy | Active movement ramp increment. Kept at top level for compatibility. |
| `acceleration_rate` | integer polling cycles | `1` | Yes | Legacy | Active movement ramp cadence. Kept at top level for compatibility. |
| `top_speed` | integer | `6` | Yes | Legacy | Legacy active movement cap/headroom. |
| `grid_size.width` | integer | `10` | Yes | Legacy | Fallback coarse jump width only when `[jump.coarse]` is missing or has zero dimensions. |
| `grid_size.height` | integer | `10` | Yes | Legacy | Fallback coarse jump height only when `[jump.coarse]` is missing or has zero dimensions. |
| `grid_mode.enabled` | boolean | `true` | Yes | Active | Enables grid mode. |
| `grid_mode.start_region` | enum string | `"current_monitor"` | Yes | Active | One of `virtual_screen`, `current_monitor`, `active_window_monitor`, `active_window_bounds`. |
| `grid_mode.width_percent` | float | `0.5` | Yes | Active | Width of the grid-mode starting region before clamping. |
| `grid_mode.height_percent` | float | `0.5` | Yes | Active | Height of the grid-mode starting region before clamping. |
| `grid_mode.center_on_cursor` | boolean | `true` | Yes | Active | Centers grid mode on the cursor. |
| `grid_mode.min_width_px` | integer pixels | `80` | Yes | Active | Minimum grid-mode region width. |
| `grid_mode.min_height_px` | integer pixels | `80` | Yes | Active | Minimum grid-mode region height. |
| `grid_mode.move_cursor_each_step` | boolean | `true` | Yes | Active | Moves cursor as grid-mode selections narrow. |
| `grid_mode.line_visible` | boolean | `true` | Yes | Active | Shows grid-mode guide line. |
| `grid_mode.show_direction_labels` | boolean | `true` | Yes | Active | Shows grid-mode direction labels. |
| `wheel.default_speed` | integer | `3` | Yes | Active | Base wheel speed. |
| `wheel.min_speed` | integer | `1` | Yes | Active | Lower bound for wheel speed changes. |
| `wheel.max_speed` | integer | `12` | Yes | Active | Upper bound for wheel speed changes. |
| `wheel.speed_step` | integer | `1` | Yes | Active | Increment used by wheel speed up/down actions. |
| `wheel.tick_interval` | integer milliseconds | `8` | Yes | Active | Wheel repeat interval. |
| `wheel.speed_indicator_ms` | integer milliseconds | `700` | Yes | Active | Duration for wheel-speed feedback. |
| `wheel.vertical_multiplier` | integer | `1` | Yes | Active | Vertical wheel multiplier. |
| `wheel.horizontal_multiplier` | integer | `1` | Yes | Active | Horizontal wheel multiplier. |
| `wheel_profiles.*.default_speed` | integer | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.min_speed` | integer | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.max_speed` | integer | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.speed_step` | integer | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.tick_interval` | integer milliseconds | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.speed_indicator_ms` | integer milliseconds | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.vertical_multiplier` | integer | none | Yes | Active | Optional named wheel profile override. |
| `wheel_profiles.*.horizontal_multiplier` | integer | none | Yes | Active | Optional named wheel profile override. |
| `scroll_mode.enabled` | boolean | `false` | Yes | Active | Enables movement-to-wheel remapping while modifier is held. |
| `scroll_mode.modifier_action` | action string | `"scroll_modifier"` | Yes | Active | Continuous action that activates scroll mode. |
| `scroll_mode.exit_on_click` | boolean | `true` | Yes | Active | Releasing scroll modifier on click for deterministic exit behavior. |
| `scroll_mode.exclusive_with_jump_mode` | boolean | `true` | Yes | Active | Reserved/experimental parse-only toggle; not currently enforced in runtime mode arbitration. |
| `scroll_mode.exclusive_with_grid_mode` | boolean | `true` | Yes | Active | Reserved/experimental parse-only toggle; not currently enforced in runtime mode arbitration. |
| `scroll_mode.exclusive_with_ui_hint_mode` | boolean | `true` | Yes | Active | Reserved exclusivity toggle for UI hint mode transitions. |
| `jump.mode` | enum string | `"precision"` | Yes | Active | `single` uses coarse only; `precision` can use coarse, fine, and precise stages. |
| `jump.cursor_between_stages` | enum string | `"none"` | Yes | Active | One of `none`, `move_to_region_center`, `preview_only`, `warp_and_continue`. |
| `jump.start_region` | enum string | `"current_monitor"` | Yes | Active | One of `virtual_screen`, `current_monitor`, `active_window_monitor`, `active_window_bounds`. |
| `jump.preview_edge_behavior` | enum string | `"clamp"` | Yes | Preview-related | Default preview edge policy for jump stages. |
| `jump.hints.selection_keys` | string | `"ABCDEFGHIJKLMNOPQRSTUVWXYZ"` | Yes | Active | Keys used to generate jump labels. Normalized to uppercase, whitespace is removed, duplicates are rejected. |
| `jump.visuals.selected_region_outline` | boolean | `true` | Yes | Active | Draws the selected jump region outline. |
| `jump.visuals.preview_outline` | boolean | `true` | Yes | Preview-related | Draws the preview outline. |
| `jump.visuals.active_grid_outline` | boolean | `true` | Yes | Active | Draws the active grid outline. |
| `jump.visuals.cell_centers` | boolean | `false` | Yes | Active | Draws cell center markers. |
| `jump.visuals.final_crosshair` | boolean | `true` | Yes | Active | Draws the final crosshair. |
| `jump.coarse.enabled` | boolean | `true` | Yes | Active | Enables the coarse stage. |
| `jump.coarse.width` | integer | `10` via `grid_size.width` fallback unless set | Yes | Active | Canonical coarse grid width. Replaces `grid_size.width` for new configs. |
| `jump.coarse.height` | integer | `10` via `grid_size.height` fallback unless set | Yes | Active | Canonical coarse grid height. Replaces `grid_size.height` for new configs. |
| `jump.coarse.aim_point` | enum string | `"center"` | Yes | Active | One of `center`, `top_left`, `top_right`, `bottom_left`, `bottom_right`, `custom_offset`. |
| `jump.coarse.aim_offset_x_px` | integer pixels | `0` | Yes | Active | X offset used with `custom_offset`. |
| `jump.coarse.aim_offset_y_px` | integer pixels | `0` | Yes | Active | Y offset used with `custom_offset`. |
| `jump.coarse.target_region_mode` | enum string | `"exact_region"` | Yes | Preview-related | One of `exact_region`, `region_with_context`, `expanded_target`, `cursor_centered_zoom`. Invalid values warn and fall back. |
| `jump.coarse.target_margin_percent` | integer percent | `0` | Yes | Preview-related | Expands the selectable target region for the next stage. |
| `jump.coarse.visual_context_margin_percent` | integer percent | `0` | Yes | Preview-related | Expands preview context without expanding the selectable region. |
| `jump.coarse.zoom_scale` | float | `1.0` | Yes | Preview-related | Scales opaque preview rendering only; it does not change grid math. |
| `jump.coarse.preview_edge_behavior` | enum string | inherited from `jump.preview_edge_behavior` | Yes | Preview-related | Stage override for preview edge policy. |
| `jump.coarse.labels.font_scale` | float | `1.0` | Yes | Active | Label font scale for the coarse stage. |
| `jump.coarse.labels.center_marker` | boolean | `false` | Yes | Active | Draws label center markers for the coarse stage. |
| `jump.coarse.labels.separators` | boolean | `true` | Yes | Active | Draws label separators for the coarse stage. |
| `jump.coarse.labels.hide_threshold_px` | integer pixels | `0` | Yes | Active | Hides labels below this cell-size threshold. |
| `jump.fine.enabled` | boolean | `true` | Yes | Active | Enables the fine stage. |
| `jump.fine.width` | integer | `5` in code, `8` in checked-in config | Yes | Active | Fine grid width. |
| `jump.fine.height` | integer | `5` in code, `8` in checked-in config | Yes | Active | Fine grid height. |
| `jump.fine.aim_point` | enum string | `"center"` | Yes | Active | Same values as coarse. |
| `jump.fine.aim_offset_x_px` | integer pixels | `0` | Yes | Active | X offset used with `custom_offset`. |
| `jump.fine.aim_offset_y_px` | integer pixels | `0` | Yes | Active | Y offset used with `custom_offset`. |
| `jump.fine.target_region_mode` | enum string | `"exact_region"` | Yes | Preview-related | Same values as coarse. Invalid values warn and fall back. |
| `jump.fine.target_margin_percent` | integer percent | `0` | Yes | Preview-related | Expands the selectable target region for the next stage. |
| `jump.fine.visual_context_margin_percent` | integer percent | `0` in code, `1` in checked-in config | Yes | Preview-related | Expands preview context without expanding the selectable region. |
| `jump.fine.zoom_scale` | float | `1.0` in code, `1.5` in checked-in config | Yes | Preview-related | Scales opaque preview rendering only. |
| `jump.fine.preview_edge_behavior` | enum string | inherited from `jump.preview_edge_behavior` | Yes | Preview-related | Stage override for preview edge policy. |
| `jump.fine.labels.font_scale` | float | `1.0` | Yes | Active | Label font scale for the fine stage. |
| `jump.fine.labels.center_marker` | boolean | `false` | Yes | Active | Draws label center markers for the fine stage. |
| `jump.fine.labels.separators` | boolean | `true` | Yes | Active | Draws label separators for the fine stage. |
| `jump.fine.labels.hide_threshold_px` | integer pixels | `0` | Yes | Active | Hides labels below this cell-size threshold. |
| `jump.precise.enabled` | boolean | `false` | Yes | Active | Enables the optional third precision stage. |
| `jump.precise.width` | integer | `3` in code, `5` in checked-in config | Yes | Active | Precise grid width. |
| `jump.precise.height` | integer | `3` in code, `5` in checked-in config | Yes | Active | Precise grid height. |
| `jump.precise.aim_point` | enum string | `"center"` | Yes | Active | Same values as coarse. |
| `jump.precise.aim_offset_x_px` | integer pixels | `0` | Yes | Active | X offset used with `custom_offset`. |
| `jump.precise.aim_offset_y_px` | integer pixels | `0` | Yes | Active | Y offset used with `custom_offset`. |
| `jump.precise.target_region_mode` | enum string | `"exact_region"` | Yes | Preview-related | Same values as coarse. Invalid values warn and fall back. |
| `jump.precise.target_margin_percent` | integer percent | `0` | Yes | Preview-related | Expands the selectable target region for the next stage. |
| `jump.precise.visual_context_margin_percent` | integer percent | `5` | Yes | Preview-related | Expands preview context without expanding the selectable region. |
| `jump.precise.zoom_scale` | float | `2.5` | Yes | Preview-related | Scales opaque preview rendering only. |
| `jump.precise.preview_edge_behavior` | enum string | inherited from `jump.preview_edge_behavior` | Yes | Preview-related | Stage override for preview edge policy. |
| `jump.precise.labels.font_scale` | float | `1.0` | Yes | Active | Label font scale for the precise stage. |
| `jump.precise.labels.center_marker` | boolean | `false` | Yes | Active | Draws label center markers for the precise stage. |
| `jump.precise.labels.separators` | boolean | `true` | Yes | Active | Draws label separators for the precise stage. |
| `jump.precise.labels.hide_threshold_px` | integer pixels | `0` | Yes | Active | Hides labels below this cell-size threshold. |
| `jump.profiles.*.mode` | enum string | inherited | Yes | Active | Named jump profile override. |
| `jump.profiles.*.cursor_between_stages` | enum string | inherited | Yes | Active | Named jump profile override. |
| `jump.profiles.*.start_region` | enum string | inherited | Yes | Active | Named jump profile override. |
| `jump.profiles.*.preview_edge_behavior` | enum string | inherited | Yes | Preview-related | Named jump profile preview override. |
| `jump.profiles.*.visuals.*` | booleans | inherited | Yes | Active | Named jump profile visual overrides. |
| `jump.profiles.*.coarse.*` | stage fields | inherited | Yes | Active | Named jump profile coarse-stage overrides. |
| `jump.profiles.*.fine.*` | stage fields | inherited | Yes | Active | Named jump profile fine-stage overrides. |
| `jump.profiles.*.precise.*` | stage fields | inherited | Yes | Active | Named jump profile precise-stage overrides. |
| `final_adjust.enabled` | boolean | `false` | Yes | Active | Pauses after completed jumps for keyboard nudging. |
| `final_adjust.small_step_px` | integer pixels | `1` | Yes | Active | Small final-adjust nudge. |
| `final_adjust.large_step_px` | integer pixels | `10` | Yes | Active | Large final-adjust nudge. |
| `final_adjust.modifier_key` | key string | `"Shift"` | Yes | Active | Modifier for large final-adjust nudges. |
| `final_adjust.confirm_key` | key string | `"Enter"` | Yes | Active | Confirms final adjust. |
| `final_adjust.cancel_key` | key string | `"Escape"` | Yes | Active | Cancels final adjust. |
| `final_adjust.back_key` | key string | `"Backspace"` | Yes | Active | Backs out of final adjust. |
| `final_adjust.show_hint` | boolean | `true` | Yes | Active | Shows final-adjust hint text. |
| `status_overlay.visibility` | enum string | `"visible"` | Yes | Active | Status overlay visibility. |
| `status_overlay.mode` | enum string | `"minimal"` | Yes | Active | Status overlay display mode. |
| `status_overlay.positioning` | enum string | `"cursor"` | Yes | Active | Status overlay placement. |
| `status_overlay.flash_duration_ms` | integer milliseconds | `700` | Yes | Active | Status overlay flash duration. |
| `status_overlay.fields.*` | booleans | `true` | Yes | Active | Toggles individual status fields (including `surgical`). |
| `tooltip_overlay.enabled` | boolean | `true` | Yes | Active | Enables tooltip/help overlays. |
| `tooltip_overlay.show_temporary_tooltips` | boolean | `true` | Yes | Active | Enables temporary runtime tooltips. |
| `tooltip_overlay.show_help` | boolean | `true` | Yes | Active | Enables the help overlay. |
| `tooltip_overlay.positioning` | enum string | `"cursor"` | Yes | Active | Temporary tooltip placement. |
| `tooltip_overlay.offset_x` | integer pixels | `18` | Yes | Active | Temporary tooltip X offset. |
| `tooltip_overlay.offset_y` | integer pixels | `18` | Yes | Active | Temporary tooltip Y offset. |
| `tooltip_overlay.duration_ms` | integer milliseconds | `900` | Yes | Active | Temporary tooltip duration. |
| `tooltip_overlay.help_positioning` | enum string | `"center"` | Yes | Active | Help overlay placement. |
| `tooltip_overlay.help_width` | integer pixels | `420` | Yes | Active | Help overlay width before clamping. |
| `tooltip_overlay.help_max_bindings` | integer | `40` | Yes | Active | Maximum bindings shown in help. |
| `tooltip_overlay.events.*` | booleans | `true` | Yes | Active | Enables tooltip event categories (including `surgical`). |
| `ui_hints.enabled` | boolean | `true` | Yes | Active | Enables UI hint mode and query workflow. |
| `ui_hints.debug` | boolean | `false` | Yes | Active | Enables UI hint debug logging. |
| `ui_hints.debug_fake_targets` | boolean | `false` | Yes | Active | Uses deterministic fake targets instead of UIA query results. |
| `ui_hints.selection_keys` | string chars | `"ABCDEFGHIJKLMNOPQRSTUVWXYZ"` | Yes | Active | Ordered key alphabet used for generated hint labels. |
| `ui_hints.label_length` | integer | `2` | Yes | Active | Starting label length for hint tokens. |
| `ui_hints.overflow_behavior` | enum string | `"increase_length"` | Yes | Active | Overflow strategy when labels are exhausted (`increase_length`, `cap`). |
| `ui_hints.max_hints` | integer | `400` | Yes | Active | Maximum number of hints to generate. |
| `ui_hints.min_hint_spacing_px` | integer pixels | `32` | Yes | Active | Minimum spacing between hints. |
| `ui_hints.include_thread_windows` | boolean | `true` | Yes | Active | Includes same-thread windows in candidate collection. |
| `ui_hints.include_owned_popups` | boolean | `true` | Yes | Active | Includes owned popup windows in candidate collection. |
| `ui_hints.query_strategy` | enum string | `"descendants"` | Yes | Active | Query traversal strategy (`descendants`, `children_then_descendants`). |
| `ui_hints.min_targets_before_fallback` | integer | `8` | Yes | Active | Minimum children matches before fallback to descendants when using children-first strategy. |
| `ui_hints.query_timeout_ms` | integer milliseconds | `1200` | Yes | Active | Timeout for async UIA queries before mode exits. |
| `ui_hints.target_point` | enum string | `"clickable_point"` | Yes | Active | Target point choice for hint selection (`clickable_point`, `center`). |
| `ui_hints.after_select` | enum string | `"move"` | Yes | Active | Post-selection behavior (`move`, `move_and_left_click`). |
| `ui_hints.overlay.font_scale` | float | `1.0` | Yes | Active | UI hint overlay font scaling factor. |
| `ui_hints.overlay.offset_x` | integer pixels | `0` | Yes | Active | UI hint overlay X offset. |
| `ui_hints.overlay.offset_y` | integer pixels | `0` | Yes | Active | UI hint overlay Y offset. |
| `ui_hints.overlay.dim_non_matching` | boolean | `true` | Yes | Active | Dims hints that do not match current prefix. |
| `ui_hints.overlay.show_background` | boolean | `true` | Yes | Active | Draws a background fill behind hint labels. |
| `ui_hints.overlay.show_border` | boolean | `true` | Yes | Active | Draws borders around hint labels. |
| `edge_jump.offset_px` | integer pixels | `1` | Yes | Active | Offset used by edge-jump actions. |
| `edge_jump.use_work_area` | boolean | `false` | Yes | Active | Uses monitor work area instead of full bounds. |
| `window_jump.enabled` | boolean | `true` | Yes | Active | Enables foreground-window snap actions. |
| `window_jump.use_extended_frame_bounds` | boolean | `true` | Yes | Active | Uses DWM extended frame bounds when available. |
| `window_jump.edge_offset_px` | integer pixels | `1` | Yes | Active | Inset offset for window edge snap targets. |
| `window_jump.titlebar_y_offset_px` | integer pixels | `10` | Yes | Active | Y offset from window top for titlebar snap target. |
| `window_jump.clamp_to_window` | boolean | `true` | Yes | Active | Clamps derived targets into foreground window bounds. |
| `jump.move_cursor_after_each_stage` | boolean | none | Yes | Deprecated alias | Replace with `jump.cursor_between_stages`. `true` maps to `move_to_region_center`; `false` maps to `none`. |
| `jump.<stage>.preview_margin_percent` | integer percent | none | Yes | Deprecated alias | Replace with `jump.<stage>.visual_context_margin_percent`. Used only when the new field is absent. |
| `system_bindings.polling_rate` | integer milliseconds | none | No | Legacy | Mis-scoped old path. Move to top-level `polling_rate`. |
| `system_bindings.grid_size` | table | none | No | Legacy | Mis-scoped old path. Move to top-level `grid_size`, or preferably to `jump.coarse.width` and `jump.coarse.height`. |
| `grid_mode.starting_speed` | integer | none | No | Legacy | Mis-scoped old path. Move to `mouse_speed.default_speed`. |
| `grid_mode.acceleration` | integer | none | No | Legacy | Mis-scoped old path. Move to top-level `acceleration`. |
| `grid_mode.acceleration_rate` | integer | none | No | Legacy | Mis-scoped old path. Move to top-level `acceleration_rate`. |
| `grid_mode.top_speed` | integer | none | No | Legacy | Mis-scoped old path. Move to top-level `top_speed`. |


### Slow mouse strategy selection

Slow mode is a separate held-movement tier and is independent from normal `[mouse_speed]` settings. Use `[mouse_speed]` for your regular movement baseline and use `[slow_mouse]` only for the temporary precision tier triggered by the `slow_mouse` action.

- `fixed`: Uses `slow_mouse.fixed_speed` directly (then applies `min_speed`/`max_speed` clamps). Best when you want a predictable precision speed regardless of your current normal speed or profile.
- `multiplier`: Uses `current_normal_speed * slow_mouse.multiplier` (then clamps). Best when you want slow mode to scale proportionally with whichever normal speed is currently active.
- `subtract`: Uses `current_normal_speed - slow_mouse.subtract_speed` (then clamps). Best when you want to keep the current speed feel but step down by a consistent amount.

If unsure, start with `fixed` for stable precision behavior, then switch to `multiplier` when you use multiple movement profiles and want consistent relative slowdown.

## Critical Examples

```toml
polling_rate = 8
grid_size = { width = 10, height = 10 } # legacy fallback only

[mouse_speed]
default_speed = 1

[jump.hints]
selection_keys = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

[jump.coarse]
width = 10
zoom_scale = 1.0

[jump.fine]
width = 8

[jump.precise]
enabled = false
```

Use `jump.coarse.width` and `jump.coarse.height` for new coarse jump sizing. Keep `grid_size` only when preserving compatibility with older configs that do not define `[jump.coarse]`.


## UI Hints Configuration

`[ui_hints]` controls UI Automation based hint discovery and label generation for `ui_hint_mode` / `show_ui_hints`.

| Path | Type | Default | Notes |
| --- | --- | --- | --- |
| `ui_hints.enabled` | bool | `true` | Enables the mode and query workflow. |
| `ui_hints.selection_keys` | string | `"ABCDEFGHIJKLMNOPQRSTUVWXYZ"` | Normalized to uppercase unique characters in-order. Empty/invalid values fall back to default alphabet. |
| `ui_hints.label_length` | integer | `2` | Clamped to `1..=5`. |
| `ui_hints.overflow_behavior` | enum | `"increase_length"` | Supported: `increase_length` (current runtime behavior). |
| `ui_hints.max_hints` | integer | `400` | Clamped to `1..=2000`; hard cap on rendered/selectable hints. |
| `ui_hints.min_hint_spacing_px` | integer px | `32` | Clamped to `0..=400`; dedupes nearby candidates to reduce noisy nested/duplicate controls. |
| `ui_hints.include_thread_windows` | bool | `true` | Include same-thread windows during candidate collection. |
| `ui_hints.include_owned_popups` | bool | `true` | Include owned popup windows (menus/dropdowns/tooltips) in candidate collection. |
| `ui_hints.query_strategy` | enum | `"descendants"` | Supported: `descendants`, `children_then_descendants`. |
| `ui_hints.min_targets_before_fallback` | integer | `8` | Clamped to `1..=5000`; applies to `children_then_descendants` fallback threshold. |
| `ui_hints.query_timeout_ms` | integer ms | `1200` | Clamped to `100..=30000`; query aborts if deadline is exceeded. |
| `ui_hints.target_point` | enum | `"clickable_point"` | Supported: `clickable_point` (current runtime behavior). |
| `ui_hints.after_select` | enum | `"move"` | Supported: `move` (moves cursor only). |

`[ui_hints.overlay]` controls hint label rendering:

| Path | Type | Default | Notes |
| --- | --- | --- | --- |
| `ui_hints.overlay.font_scale` | float | `1.0` | Clamped to `0.5..=4.0`. Scales label text in UI hint overlay. |

### UI Hints example

```toml
[ui_hints]
enabled = true
selection_keys = "ASDFJKL;"
label_length = 2
overflow_behavior = "increase_length"
max_hints = 250
min_hint_spacing_px = 28
include_thread_windows = true
include_owned_popups = true
query_strategy = "descendants"
min_targets_before_fallback = 8
query_timeout_ms = 1200
target_point = "clickable_point"
after_select = "move"

[ui_hints.overlay]
font_scale = 1.1
```

### Normalization notes

- `selection_keys` removes whitespace, uppercases alpha keys, and removes duplicates while preserving first occurrence order.
- Values outside supported ranges are normalized and logged as config warnings so the mode remains usable.
- Duplicate and deeply nested UIA controls are common in complex apps; increase `min_hint_spacing_px` to reduce visual crowding.
- When many controls are visible (for example browsers/Electron apps), results may be capped by `max_hints`; consider larger `selection_keys` and spacing tuning before raising the cap aggressively.

## UI Hints limitations

- Weak UIA trees: each app exposes a different UIA tree, and some expose very little actionable metadata, so discoverability can vary widely.
- Elevation boundary behavior: an elevated foreground app can block or reduce non-elevated UIA visibility (and vice versa). Run Multi MouseMover at a matching integrity level when possible.
- Custom-rendered/game UIs: many non-standard UI toolkits and game engines do not provide robust UIA metadata, so hints may be partial or absent.
- High-density element apps: browsers/Electron apps can expose very large control trees; tune `min_hint_spacing_px` and `max_hints` to keep overlays readable.

## Migration Notes

| Old path | New path | Notes |
| --- | --- | --- |
| `system_bindings.polling_rate` | `polling_rate` | `polling_rate` is read only at top level. |
| `system_bindings.grid_size.width` | `jump.coarse.width` | Prefer the canonical jump stage path. Top-level `grid_size.width` remains only as a legacy fallback. |
| `system_bindings.grid_size.height` | `jump.coarse.height` | Prefer the canonical jump stage path. Top-level `grid_size.height` remains only as a legacy fallback. |
| `grid_size.width` | `jump.coarse.width` | Replace when `[jump.coarse]` is present. |
| `grid_size.height` | `jump.coarse.height` | Replace when `[jump.coarse]` is present. |
| `grid_mode.starting_speed` | `mouse_speed.default_speed` | `starting_speed` is no longer a grid-mode field. |
| `starting_speed` | `mouse_speed.default_speed` | Top-level `starting_speed` remains a compatibility seed. |
| `grid_mode.acceleration` | `acceleration` | Kept at top level. |
| `grid_mode.acceleration_rate` | `acceleration_rate` | Kept at top level. |
| `grid_mode.top_speed` | `top_speed` | Kept at top level. |
| `jump.move_cursor_after_each_stage = true` | `jump.cursor_between_stages = "move_to_region_center"` | Modern field wins when both are present. |
| `jump.move_cursor_after_each_stage = false` | `jump.cursor_between_stages = "none"` | Modern field wins when both are present. |
| `jump.coarse.preview_margin_percent` | `jump.coarse.visual_context_margin_percent` | Deprecated alias used only when the new field is absent. |
| `jump.fine.preview_margin_percent` | `jump.fine.visual_context_margin_percent` | Deprecated alias used only when the new field is absent. |
| `jump.precise.preview_margin_percent` | `jump.precise.visual_context_margin_percent` | Deprecated alias used only when the new field is absent. |

## Step move configuration

`[step_move]` controls one-shot pixel nudges via directional actions.

```toml
[step_move]
enabled = true
small_step_px = 20
normal_step_px = 80
large_step_px = 240
clamp_mode = "virtual_screen" # none | virtual_screen | current_monitor | current_work_area
show_tooltip = true
```

Suggested bindings:
- `Alt+E/S/D/F` => `step_move_up/left/down/right` (normal tier)
- `Alt+Ctrl+E/S/D/F` => `step_move_small_*`
- `Alt+Shift+E/S/D/F` => `step_move_large_*`

Note: many apps reserve `Alt` accelerators for menu/navigation. If a binding is intercepted, prefer switching to a less contended modifier combo.

### `[position_history]`

Configures saved mouse-position history and selection labels.

```toml
[position_history]
enabled = true
max_positions = 20
selection_keys = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
label_length = 2
show_numbers = false
```

### `[bookmarks]`

Configures bookmark mode and bookmark slot behavior. Bookmark JSON path resolution is relative to the resolved `config.toml` path when `bookmarks.file` is not absolute.

```toml
[bookmarks]
enabled = true
file = "bookmarks.json"
slot_count = 9            # clamped to 1..99
show_tooltips = true
desktop_behavior = "focus_anchor_window"
desktop_switch_wait_ms = 150   # clamped to 0..3000
require_desktop_switch_success = false
cancel_key = "Escape"           # invalid values fallback to Escape
clear_modifier_key = "Backspace" # invalid values fallback to Backspace
coordinate_policy = "clamp_to_virtual_screen"
```

`desktop_behavior` currently supports `focus_anchor_window` (invalid values fall back to this). `coordinate_policy` currently supports `clamp_to_virtual_screen` (invalid values fall back to this).

### Surgical mode

| Path | Type | Default | Required | Status | Description |
| --- | --- | --- | --- | --- | --- |
| `surgical_mode.enabled` | boolean | `false` | Yes | Active | Enables fixed-speed surgical movement tier. |
| `surgical_mode.speed_px` | integer | `1` | Yes | Active | Fixed movement speed used when `surgical_mode` action is held. |
| `surgical_mode.zoom_enabled` | boolean | `false` | Yes | Active | Experimental: tracks surgical zoom intent; full rendered zoom viewport is not currently shown. |
| `surgical_mode.zoom_scale` | float | `2.0` | Yes | Active | Zoom factor for surgical overlay. |
| `surgical_mode.zoom_size_px` | integer | `180` | Yes | Active | Surgical overlay square size in pixels. |
| `surgical_mode.overlay_offset_x` | integer | `24` | Yes | Active | Overlay horizontal offset from cursor. |
| `surgical_mode.overlay_offset_y` | integer | `24` | Yes | Active | Overlay vertical offset from cursor. |


### Surgical behavior semantics

`SurgicalMode` is a **held precision modifier**: movement requires both the surgical key and a movement key. Holding surgical alone produces no cursor movement. While active, movement runs at fixed `surgical_mode.speed_px` with no acceleration ramp.
