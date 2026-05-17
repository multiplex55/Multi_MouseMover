# Smoke Test: Input Pipeline

Use this checklist when changing keyboard hook setup, event routing, command execution,
movement ticking, or overlay message pumping.

## Setup

- [ ] Build the current tree:

```powershell
cargo build
```

- [ ] Start the app from the repository root with diagnostics enabled:

```powershell
$env:MULTI_MOUSEMOVER_DEBUG = "1"
cargo run
```

- [ ] Confirm `config.toml` is the active configuration and still contains the
      default bindings used below: `A`, `D`, `Space`, and `Escape`.
- [ ] Run the app in a context where the global keyboard hook can install. On many
      Windows systems this means an elevated terminal.

## Startup Expectations

- [ ] Startup reaches the main loop.
- [ ] Expected log fragments:

```text
🚀 Multi MouseMover (multi_mousemover) Program Start!
✅ Config Loaded
✅ Key Bindings Initialized
🔹 Attempting to Get Module Handle...
✅ Module Handle Retrieved
🔹 Setting Up Keyboard Hook...
✅ Keyboard Hook Installed Successfully!
🔄 Entering Main Event Loop...
```

- [ ] If overlay creation succeeds, expect:

```text
✅ Overlay Initialized Successfully
```

- [ ] If overlay creation fails, the input pipeline can still be tested when the
      app continues after:

```text
Overlay disabled due to initialization failure
```

## Heartbeat Expectations

- [ ] Leave the app idle for at least two seconds.
- [ ] Expected log fragment, once per second while diagnostics are enabled:

```text
[heartbeat] hook_seen=
```

- [ ] Each heartbeat line should include all counters:

```text
hook_seen=
hook_decoded=
hook_swallowed=
queue=
commands=
ticks=
loops=
messages=
```

- [ ] While idle, `loops` should be positive. `ticks` can be `0` until movement is
      held. `messages` can vary with normal Windows message traffic.

## Interaction Checklist

- [ ] Press and hold `A`, then release it.
- [ ] Expected key down/up routing and command flow:

```text
[routing] key=A state=down action=MoveLeft
[command] KeyAction action=MoveLeft state=down
[routing] key=A state=up action=MoveLeft
[command] KeyAction action=MoveLeft state=up
```

- [ ] While `A` is held, expect movement diagnostics showing active movement:

```text
[DEBUG] Mode: Active
Active Keys:
MoveLeft
Movement: true
```

- [ ] Press and hold `D`.
- [ ] Expected `D` key down flow:

```text
[routing] key=D state=down action=MoveRight
[command] KeyAction action=MoveRight state=down
```

- [ ] While `D` is held, a following heartbeat should show `ticks` greater than
      `0`.
- [ ] Tap `Space`.
- [ ] Expected click action flow:

```text
[routing] key=Space state=down action=LeftClick
[command] KeyAction action=LeftClick state=down
[DEBUG] Left Click Pressed!
```

- [ ] A `Space` key-up should be routed too:

```text
[routing] key=Space state=up action=LeftClick
[command] KeyAction action=LeftClick state=up
```

- [ ] Press `Escape`.
- [ ] Expected exit flow:

```text
[routing] key=Escape state=down action=Exit
[command] Exit
Exiting
```

## Counter Interpretation

- [ ] `hook_seen > 0` means the low-level hook callback is receiving keyboard
      messages.
- [ ] `hook_decoded > 0` means received hook messages map to known `VirtualKey`
      values.
- [ ] `hook_seen > hook_decoded` usually means some received keys are not decoded
      by `VirtualKey::from_vk_code`.
- [ ] `queue > 0` means decoded key events are leaving the hook path and being
      processed by the main loop.
- [ ] `commands > 0` means queued events are routing to `AppCommand` values and
      command execution is running.
- [ ] `ticks > 0` means a movement key is active and `tick_movement` is producing
      movement during heartbeat sampling.
- [ ] `hook_seen > 0`, `hook_decoded > 0`, `queue = 0` points at queue draining or
      main-loop starvation.
- [ ] `queue > 0`, `commands = 0` points at routing, active-mode state, or binding
      lookup.
- [ ] `commands > 0`, `ticks = 0` is valid for non-movement actions such as
      `Space` and `Escape`; it is a failure for held movement keys like `A` or `D`.
- [ ] `ticks > 0` with no cursor movement points at the Enigo mouse movement path,
      OS permissions, or coordinate handling after command routing has succeeded.

## Known Failure Signature: Message-Flood Starvation

- [ ] Suspect message-flood starvation when heartbeat lines show high `messages`
      with `ticks=0` while a movement key is held.
- [ ] A strong signature is repeated heartbeat output shaped like:

```text
[heartbeat] hook_seen=<increasing> hook_decoded=<increasing> hook_swallowed=<any> queue=<low-or-zero> commands=<low-or-zero> ticks=0 loops=<positive> messages=<high>
```

- [ ] First triage pointers:
  - Check `MAX_MESSAGES_PER_TICK` and `drain_windows_messages` in `src/main.rs`.
  - Verify the main loop still calls `process_queued_key_events` after draining
    messages.
  - Confirm `polling_rate` in `config.toml` is nonzero and reasonable.
  - Check whether overlay updates or window messages are producing a continuous
    stream that keeps `messages` high.
  - If `hook_seen` and `hook_decoded` increment but `queue` stays at `0`, inspect
    the queue handoff in `keyboard_hook` and `AppState::enqueue_key_event`.
