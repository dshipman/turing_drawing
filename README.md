# Turing Drawings (Rust + iced)

A local desktop port of [Turing Drawings](https://maximecb.github.io/Turing-Drawings/) by Maxime Chevalier-Boisvert.

Randomly generated 2D Turing machines draw generative art on a wrapping grid. Multiple machines can share the same canvas: they must use the same number of states and symbols, but each has its own rules and start position. This reimplementation keeps the original transition-table layout, action quirks, and color palette. Original website `#hash` encodings still load (start position defaults to `(0,0)`).

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
- **Random** — generate new rules and start positions for every machine
- **Restart** — clear the grid and send each head back to its start without changing rules
- **Add machine** — append another random machine and reset the drawing
- **Remove** — drop a machine (not the last one) and reset the drawing
- **Fullscreen** / **F11** — toggle OS-level window fullscreen
- **Drawing** — scales to fill available space as you resize the window. Double-click the drawing to show only the drawing (maximized in the window); double-click again to restore controls. **Escape** leaves drawing-only mode and exits fullscreen.
- **Shareable encoding** — one field per machine. Format: `numStates,numSymbols,startX,startY,` then the flat transition table. Original `#hash` URLs (no start fields) load with start `(0,0)`. A leading `#` is stripped on load. With more than one machine, a loaded encoding must match the current state/symbol counts.

## Credit

Original concept and JavaScript demo: [maximecb/Turing-Drawings](https://github.com/maximecb/Turing-Drawings) (Modified BSD License), Copyright © 2012 Maxime Chevalier-Boisvert.

This Rust port is a from-scratch reimplementation and is not a copy of the original source files.
