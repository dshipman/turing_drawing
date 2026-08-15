# Turing Drawings (Rust + iced)

A local desktop port of [Turing Drawings](https://maximecb.github.io/Turing-Drawings/) by Maxime Chevalier-Boisvert.

Randomly generated 2D Turing machines draw generative art on a wrapping grid. This reimplementation keeps the original transition-table layout, action quirks, color palette, and share-string encoding so hashes from the website load the same machines.

## Requirements

- Rust 1.88+ (iced 0.14)
- A desktop environment (macOS, Linux, or Windows)

## Run

```bash
cargo run --release
```

Debug builds work but the simulation is much slower; prefer `--release`.

## Controls

- **Num states / Num symbols** — size of the transition table (defaults: 4 / 3)
- **Random** — generate a new machine
- **Restart** — clear the grid and reset the head without changing rules
- **Shareable encoding** — copy or paste the comma-separated machine string (compatible with original `#hash` URLs; a leading `#` is stripped on load)

## Credit

Original concept and JavaScript demo: [maximecb/Turing-Drawings](https://github.com/maximecb/Turing-Drawings) (Modified BSD License), Copyright © 2012 Maxime Chevalier-Boisvert.

This Rust port is a from-scratch reimplementation and is not a copy of the original source files.
