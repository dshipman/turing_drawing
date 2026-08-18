# Turing Drawings (Rust + iced)

A local desktop port of [Turing Drawings](https://maximecb.github.io/Turing-Drawings/) by Maxime Chevalier-Boisvert.

Randomly generated 2D Turing machines draw generative art on a wrapping grid. Multiple machines can share the same canvas: they must use the same number of states and symbols, but each has its own rules, start position, and palette. This reimplementation keeps the original transition-table layout and action quirks. The tape still stores symbols so machines can read; the canvas stores baked RGB at write time and on Restart. The right-panel Palette is the canvas/init palette (Restart/Reseed only). Each machine has its own palette in its inspector; when a head writes symbol *S*, that machine's current RGB for *S* is baked into the canvas. Editing a palette afterwards does not recolor existing cells. The Classic palette matches the original colours. Original website `#hash` encodings still load (start position defaults to `(0,0)`).

## Requirements

- Rust 1.88+ (iced 0.14)
- A desktop environment (macOS, Linux, or Windows)

## Run

```bash
cargo run --release
```

Debug builds work but the simulation is much slower; prefer `--release`.

Headless CPU timings for saved presets and every resolution preset:

```bash
cargo run --release --bin profile_presets
```

## Controls

The window is a compact toolbar, a left machine pool, a centre drawing, a right panel (Canvas, Palette, Simulation), and a status bar. Those groups collapse from their headers. Selecting a machine opens its details below the machine list.

- **Num states / Num symbols** — size of every machine's transition table (defaults: 4 / 3)
- **Resolution** — size of the drawing grid (default `512 × 512`). Choose a preset (square `512`, `1024`, `2048`; 16:9 and 16:10 fullscreen sizes; MacBook Air/Pro logical and native sizes; 21:9 ultrawide) or type a custom width and height (`64`–`4096`). Changing resolution reallocates the grid, wraps start positions, and clears the drawing (same as Restart).
- **Init** — how Restart fills the canvas. **Empty** (default) is all background (symbol 0). **Uniform** picks a random symbol independently for every cell. **Gaussian** samples a bell curve per pixel (Mean and Sigma are fractions of the symbol range). **Perlin** fills the wrapping grid with smooth noise (Scale is feature size in cells; Octaves stacks detail). Changing Init or its sliders resets the drawing. **Reseed** picks a new pattern; **Restart** replays the same one (the seed is kept).
- **Palette** — colours used when Restart / Reseed fills the canvas from the current Init generator. Presets: Classic (original first-N `colorMap`), Grayscale, Sunset, Ocean, Neon. Named presets other than Classic pick evenly across the full 8-colour range so a small symbol count still reaches the bright end. **Gradient** builds colours from user-chosen start and end colours, spanning the current number of symbols (slot 0 = start / untouched background; last active slot = end). Click a start/end swatch for the colour picker (wheel, HSV, RGB); hex is on the Start/End fields. Changing Num symbols regenerates sampled / gradient colours. Each machine also has its own palette in its inspector. When a head writes symbol *S*, the canvas stores that machine's current RGB for *S* at write time. Editing a palette does not recolor cells already on the canvas.
- **Random** — generate new rules and start positions for every machine
- **Randomise** — generate new rules and start position for one machine (on that machine's row in the left pool)
- **Speed** (global) — how hard the simulation runs each frame (`0` = paused, `1` = max). This is a fraction of **Max itrs/frame**.
- **Refresh rate** — target simulation ticks per second (default `60` Hz). Choose a preset (`30`, `60`, `120`, `144`, `165`, `240`) or type a custom integer (`1`–`240`). Each tick also stops if that frame's time budget is used up, so work cannot overrun the chosen period.
- **Max itrs/frame** — cap on simulation rounds each tick at Speed `1` (default `350000`, range `1000`–`2000000`). Work also yields when the frame's time budget is spent.
- **Raster** — how the baked RGBA canvas is shown. **GPU** (default) uploads the canvas texture directly; **CPU** copies the canvas to an image widget (fallback). Machine stepping always runs on the CPU. GPU mode needs iced's wgpu backend (the desktop default). If the drawing is blank, switch to CPU.
- **Speed** (per machine) — how often that machine steps relative to the others. The slider is continuous from `−10` to `+10` (step `0.1`). `0` is the default rate (one step per round). Frequency is `10^(speed / 10)`, so `+10` is ten times more often and `−10` is ten times less often. Changing speed does not reset the drawing.
- **Restart** — refill the grid from the current Init generator and send each head back to its start without changing rules. **Shift+R** does the same (rebindable in Settings). With Empty this is a blank canvas; with Uniform / Gaussian / Perlin it restores the same seeded pattern. **Reseed** (on the Canvas panel, or a rebindable shortcut) chooses a new seed and resets.
- **Presets** — open the preset browser to **Store** the starting setup of all machines (shared state/symbol counts, canvas size, tape init kind/params/seed, each machine's name, rules, start position, speed, palette, and list order) to disk, or **Load** / **Delete** a saved preset. Loading replaces the current machines and clears the drawing (same as Restart after swapping rules). **Performance bindings** (per-machine shortcuts set by right-clicking machine buttons) are saved and loaded with presets. Canvas palette, global Speed, Refresh rate, and Max itrs/frame are not saved. Older preset files without a canvas size load at `512 × 512`; older files without machine names get `Machine 1`, `Machine 2`, …; older files without tape init start Empty; older files without machine palettes use Classic; older files without performance bindings start with none. Presets live in the app data folder (e.g. `~/Library/Application Support/turing_drawing/presets/` on macOS) as JSON; storing the same name overwrites. The list can be sorted by **Name** or **Date saved** (newest first; default).
- **Settings** — open a modal with **Defaults** and **Keybindings** tabs. Defaults are the startup values for the right-hand panel (states, symbols, canvas size, tape init kind and params, canvas palette, speed, refresh rate, max itrs/frame, raster). The canvas palette is also cloned onto the first machine at launch. A fresh tape seed is chosen at launch. Changing those controls in the inspector only affects the current session. **Global** keybindings (Restart, Reseed tape, Fullscreen, etc.) are rebindable from the Keybindings tab or by **right-clicking any non-machine button**; they are saved in `settings.json`. **Performance bindings** (Randomise, Mutate, Set start, Copy, Load, Remove for a specific machine) apply only to that machine, are cleared when it is removed, and are stored with presets when you **Store** (not in Settings). Right-click any button to set or clear its shortcut.
- **Add machine** — append another random machine (its palette starts as a copy of the canvas palette) and reset the drawing (button at the top of the machine list)
- **Remove** — drop a machine (not the last one) and reset the drawing; available in that machine's inspector
- **Fullscreen** / **F11** — toggle OS-level window fullscreen (F11 is the default binding; rebindable in Settings)
- **Drawing** — fills the centre of the window and keeps the grid's aspect ratio (letterboxed if the window does not match). Double-click the drawing to show only the drawing; double-click again to restore the chrome. **Escape** closes the button shortcut dialog (without binding Escape), then cancels Settings keybinding capture, then closes Settings, then the preset browser, then the machine inspector, then the colour picker, then leaves drawing-only mode and exits fullscreen. After setting a shortcut in the button dialog, **Return** closes it.
- **Machines** — the left pool lists each machine with an Active toggle, Speed slider, and Randomise. Inactive machines do not step. With more than one machine, drag the `::` handle on a card to reorder the list (that order is also the simulation step order; reordering does not reset the drawing). Click a name or anywhere on the card to select it and show a Name field, live state, position, palette, shareable encoding, Copy / Load / Remove below the list. Names default to `Machine N` and are kept when randomising or loading an encoding. Click the card again, press Escape, or click Close to deselect. Encoding format: `numStates,numSymbols,startX,startY,` then the flat transition table (names and palettes are not part of the encoding). Original `#hash` URLs (no start fields) load with start `(0,0)`. A leading `#` is stripped on load. With more than one machine, a loaded encoding must match the current state/symbol counts.

## Credit

Original concept and JavaScript demo: [maximecb/Turing-Drawings](https://github.com/maximecb/Turing-Drawings) (Modified BSD License), Copyright © 2012 Maxime Chevalier-Boisvert.

This Rust port is a from-scratch reimplementation and is not a copy of the original source files.
