# Development Notes

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
