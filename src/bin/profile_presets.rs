//! Headless timings for saved machine presets and canvas-size presets.
//!
//! ```bash
//! cargo run --release --bin profile_presets
//! ```

use std::time::{Duration, Instant};

use turing_drawing::palette::{fill_rgba_from_map, Palette};
use turing_drawing::preset::{self, list_presets, load_preset, sort_preset_infos, PresetSort};
use turing_drawing::program::Program;

const CHUNK: usize = 5_000;
const DEFAULT_MAX_ITRS: u64 = 350_000;
const FRAMES: u32 = 4;
const RESOLUTION_PRESETS: [(&str, usize, usize); 18] = [
    ("512 × 512", 512, 512),
    ("1024 × 1024", 1024, 1024),
    ("2048 × 2048", 2048, 2048),
    ("1280 × 720 (16:9)", 1280, 720),
    ("1920 × 1080 (16:9)", 1920, 1080),
    ("2560 × 1440 (16:9)", 2560, 1440),
    ("3840 × 2160 (16:9)", 3840, 2160),
    ("1280 × 800 (16:10)", 1280, 800),
    ("1440 × 900 (16:10)", 1440, 900),
    ("1920 × 1200 (16:10)", 1920, 1200),
    ("2560 × 1600 (16:10)", 2560, 1600),
    ("1470 × 956 (13\" Air)", 1470, 956),
    ("1512 × 982 (14\" MBP)", 1512, 982),
    ("1728 × 1117 (16\" MBP)", 1728, 1117),
    ("2560 × 1664 (13\" Air native)", 2560, 1664),
    ("2880 × 1864 (15\" Air native)", 2880, 1864),
    ("2560 × 1080 (21:9)", 2560, 1080),
    ("3440 × 1440 (21:9)", 3440, 1440),
];

struct Timings {
    sim: Duration,
    raster: Duration,
    clone: Duration,
    steps: u64,
}

fn time_frames(program: &mut Program, frames: u32) -> Timings {
    let colors = Palette::classic().colors;
    let mut pixels = vec![0u8; program.map.len() * 4];
    let mut sim = Duration::ZERO;
    let mut raster = Duration::ZERO;
    let mut clone_t = Duration::ZERO;
    let mut steps = 0u64;

    for _ in 0..frames {
        if pixels.len() != program.map.len() * 4 {
            pixels.resize(program.map.len() * 4, 0);
        }

        let start_itr = program.itr_count;
        let t0 = Instant::now();
        let mut remaining = DEFAULT_MAX_ITRS;
        while remaining > 0 {
            let chunk = CHUNK.min(remaining as usize);
            program.update(chunk);
            remaining = remaining.saturating_sub(chunk as u64);
        }
        sim += t0.elapsed();
        steps += program.itr_count.saturating_sub(start_itr);

        let t1 = Instant::now();
        fill_rgba_from_map(&program.map, &mut pixels, &colors);
        raster += t1.elapsed();

        let t2 = Instant::now();
        std::hint::black_box(pixels.clone());
        clone_t += t2.elapsed();
    }

    Timings {
        sim,
        raster,
        clone: clone_t,
        steps,
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

fn print_row(name: &str, width: usize, height: usize, machines: usize, t: &Timings, frames: u32) {
    let cells = width * height;
    let f = f64::from(frames);
    let sim = ms(t.sim) / f;
    let raster = ms(t.raster) / f;
    let clone = ms(t.clone) / f;
    let total = sim + raster + clone;
    let steps = t.steps as f64 / f;
    println!(
        "{name:<28} {width:>4}×{height:<4} {machines:>3} {cells:>10} {steps:>10.0} {sim:>8.2} {raster:>8.2} {clone:>8.2} {total:>8.2}"
    );
}

fn header() {
    println!(
        "{:<28} {:>9} {:>3} {:>10} {:>10} {:>8} {:>8} {:>8} {:>8}",
        "name", "canvas", "m", "cells", "steps", "sim ms", "rast ms", "clone ms", "total"
    );
}

fn main() {
    println!(
        "Profiling {FRAMES} frames × {DEFAULT_MAX_ITRS} itrs (chunk {CHUNK}), release timings.\n"
    );

    println!("== saved machine presets ==");
    header();
    match list_presets() {
        Ok(mut list) if !list.is_empty() => {
            sort_preset_infos(&mut list, PresetSort::Name);
            for info in list {
                match load_preset(&info.name) {
                    Ok(preset) => match Program::from_preset(&preset) {
                        Ok(mut program) => {
                            let t = time_frames(&mut program, FRAMES);
                            print_row(
                                &info.name,
                                program.width,
                                program.height,
                                program.machines.len(),
                                &t,
                                FRAMES,
                            );
                        }
                        Err(e) => eprintln!("skip {}: {e}", info.name),
                    },
                    Err(e) => eprintln!("skip {}: {e}", info.name),
                }
            }
        }
        Ok(_) => println!("(no saved presets in {})", preset_dir_hint()),
        Err(e) => eprintln!("could not list presets: {e}"),
    }

    println!("\n== resolution presets (one random 4-state / 3-symbol machine) ==");
    header();
    let mut program = Program::new_random(4, 3);
    for (label, width, height) in RESOLUTION_PRESETS {
        if let Err(e) = program.set_size(width, height) {
            eprintln!("skip {label}: {e}");
            continue;
        }
        program.reset();
        let t = time_frames(&mut program, FRAMES);
        print_row(label, width, height, program.machines.len(), &t, FRAMES);
    }
}

fn preset_dir_hint() -> String {
    preset::presets_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown presets dir)".into())
}
