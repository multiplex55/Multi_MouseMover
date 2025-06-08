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

Configuration lives in `config.toml` in the project root.  Key bindings and mouse parameters can be tweaked there.

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

Holding **Alt + E** toggles between *Active* and *Idle* modes where keybinds are processed or ignored respectively.

## Jump Mode

Press the `F` key to activate *jump mode*.  A translucent grid appears over the screen labelled with letter pairs.  Type the displayed sequence (for example `AA`, `AB`, etc.) to instantly move the cursor to that grid cell.  The grid size can be customised via the `grid_size` setting in `config.toml`.

Example sequence:

1. Hit `F` – the grid overlay appears.
2. Enter the letter pair shown in the target cell.
3. The mouse jumps to that position and the overlay hides.

## Configuration Options

`config.toml` exposes several tunables:

- `key_bindings` – mapping of keyboard keys to actions.
- `polling_rate` – delay (ms) between input polls.
- `grid_size` – width/height of the jump grid (e.g. `{width = 10, height = 10}`).
- `starting_speed` – initial mouse speed in pixels per step.
- `acceleration` and `acceleration_rate` – how quickly speed increases when holding a direction.
- `top_speed` – maximum mouse speed.

Adjust these values to suit your workflow.  After editing the file restart the application to apply changes.

## License

This project is licensed under the terms of the MIT license.  See [LICENSE](LICENSE) for details.
