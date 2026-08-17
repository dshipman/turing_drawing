# Turing Drawings (Rust + iced)

A local desktop port of [Turing Drawings](https://maximecb.github.io/Turing-Drawings/) by Maxime Chevalier-Boisvert.

Randomly generated 2D Turing machines draw generative art on a wrapping grid. Multiple machines can share the same canvas: they must use the same number of states and symbols, but each has its own rules and start position. This reimplementation keeps the original transition-table layout and action quirks; the Classic palette matches the original colours, and other palettes (including a custom gradient) can recolor drawings at render time. Original website `#hash` encodings still load (start position defaults to `(0,0)`).

## Requirements

- Rust 1.88+ (iced 0.14)
- A desktop environment (macOS, Linux, or Windows)

## Run

```bash
cargo run --release
```

Debug builds work but the simulation is much slower; prefer `--release`.

## Controls

- **Num states / Num symbols** — size of every machine's transition table (defaults: 4 / 3)
- **Palette** — recolor the drawing without changing machine rules. Presets: Classic (original), Grayscale, Sunset, Ocean, Neon. **Gradient** builds colours from user-chosen start and end colours, spanning the current number of symbols (slot 0 = start / untouched background; last active slot = end). Click a start/end swatch for a full colour picker (wheel, HSV, RGB, hex). Changing Num symbols regenerates the gradient.
- **Random** — generate new rules and start positions for every machine
- **Randomise** — generate new rules and start position for one machine (on that machine's row, and in its details)
- **Speed** (global) — how hard the simulation runs each frame (`0` = paused, `1` = max). This is a fraction of **Max itrs/frame**.
- **Refresh rate** — target simulation ticks per second (default `60` Hz). Choose a preset (`30`, `60`, `120`, `144`, `165`, `240`) or type a custom integer (`1`–`240`). Each tick also stops if that frame's time budget is used up, so work cannot overrun the chosen period.
- **Max itrs/frame** — cap on simulation rounds each tick at Speed `1` (default `350000`, range `1000`–`2000000`). Work also yields when the frame's time budget is spent.
- **Speed** (per machine) — how often that machine steps relative to the others. The slider is continuous from `−10` to `+10` (step `0.1`). `0` is the default rate (one step per round). Frequency is `10^(speed / 10)`, so `+10` is ten times more often and `−10` is ten times less often. Changing speed does not reset the drawing.
- **Restart** — clear the grid and send each head back to its start without changing rules
- **Presets** — open the preset browser to **Store** the starting setup of all machines (shared state/symbol counts, each machine's rules, start position, and speed) to disk, or **Load** / **Delete** a saved preset. Loading replaces the current machines and clears the drawing (same as Restart after swapping rules). Palette, global Speed, Refresh rate, and Max itrs/frame are not saved. Presets live in the app data folder (e.g. `~/Library/Application Support/turing_drawing/presets/` on macOS) as JSON; storing the same name overwrites. The list can be sorted by **Name** or **Date saved** (newest first; default).
- **Add machine** — append another random machine and reset the drawing
- **Remove** — drop a machine (not the last one) and reset the drawing; available in that machine's details
- **Fullscreen** / **F11** — toggle OS-level window fullscreen
- **Drawing** — scales to fill available space as you resize the window. Double-click the drawing to show only the drawing (maximized in the window); double-click again to restore controls. **Escape** closes the preset browser or machine details, then leaves drawing-only mode and exits fullscreen.
- **Machines** — the list shows each machine's name, Speed slider, and a Randomise button. Click a name to open its details (live state and position, Speed, shareable encoding, Copy / Load / Randomise / Remove). Click the name again, press Escape, or click outside the panel to close it. Encoding format: `numStates,numSymbols,startX,startY,` then the flat transition table. Original `#hash` URLs (no start fields) load with start `(0,0)`. A leading `#` is stripped on load. With more than one machine, a loaded encoding must match the current state/symbol counts.

## Credit

Original concept and JavaScript demo: [maximecb/Turing-Drawings](https://github.com/maximecb/Turing-Drawings) (Modified BSD License), Copyright © 2012 Maxime Chevalier-Boisvert.

This Rust port is a from-scratch reimplementation and is not a copy of the original source files.
