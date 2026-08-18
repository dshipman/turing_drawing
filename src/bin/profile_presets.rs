//! Headless timings for saved machine presets and canvas-size presets.
//!
//! ```bash
//! cargo run --release --bin profile_presets
//! ```

use std::time::{Duration, Instant};

use turing_drawing::preset::{self, list_presets, load_preset, sort_preset_infos, PresetSort};
use turing_drawing::program::Program;
use turing_drawing::settings::DEFAULT_MAX_ITRS;

const CHUNK: usize = 5_000;
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
    raster_full: Duration,
    raster_dirty: Duration,
    clone_full: Duration,
    clone_dirty: Duration,
    steps: u64,
}

fn time_frames(program: &mut Program, frames: u32) -> Timings {
    let mut sim = Duration::ZERO;
    let mut raster_full = Duration::ZERO;
    let mut raster_dirty = Duration::ZERO;
    let mut clone_full = Duration::ZERO;
    let mut clone_dirty = Duration::ZERO;
    let mut steps = 0u64;
    let mut dirty_copy = Vec::new();
    let row_bytes = program.width * 4;

    for _ in 0..frames {
        let start_itr = program.itr_count;
        let t0 = Instant::now();
        let mut frame_dirty: Option<turing_drawing::dirty::DirtyRect> = None;
        let mut remaining = DEFAULT_MAX_ITRS;
        while remaining > 0 {
            let chunk = CHUNK.min(remaining as usize);
            let chunk_dirty = program.update(chunk);
            if let Some(rect) = chunk_dirty {
                frame_dirty = Some(match frame_dirty {
                    Some(acc) => acc.union(rect),
                    None => rect,
                });
            }
            remaining = remaining.saturating_sub(chunk as u64);
        }
        sim += t0.elapsed();
        steps += program.itr_count.saturating_sub(start_itr);

        let t1 = Instant::now();
        let pixels_full = program.canvas.clone();
        raster_full += t1.elapsed();

        let t1b = Instant::now();
        if let Some(rect) = frame_dirty {
            let need = rect.width as usize * rect.height as usize * 4;
            if dirty_copy.len() < need {
                dirty_copy.resize(need, 0);
            }
            let mut out = 0usize;
            let x_off = rect.x as usize * 4;
            for y in rect.y as usize..(rect.y + rect.height) as usize {
                let src = y * row_bytes + x_off;
                let n = rect.width as usize * 4;
                dirty_copy[out..out + n].copy_from_slice(&program.canvas[src..src + n]);
                out += n;
            }
            std::hint::black_box(&dirty_copy[..need]);
        }
        raster_dirty += t1b.elapsed();

        let t2 = Instant::now();
        std::hint::black_box(pixels_full.clone());
        clone_full += t2.elapsed();

        let t2b = Instant::now();
        if let Some(rect) = frame_dirty {
            let n = rect.width as usize * rect.height as usize * 4;
            std::hint::black_box(dirty_copy[..n].to_vec());
        }
        clone_dirty += t2b.elapsed();
    }

    Timings {
        sim,
        raster_full,
        raster_dirty,
        clone_full,
        clone_dirty,
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
    let raster_full = ms(t.raster_full) / f;
    let raster_dirty = ms(t.raster_dirty) / f;
    let clone_full = ms(t.clone_full) / f;
    let clone_dirty = ms(t.clone_dirty) / f;
    let total_full = sim + raster_full + clone_full;
    let total_dirty = sim + raster_dirty + clone_dirty;
    let steps = t.steps as f64 / f;
    println!(
        "{name:<28} {width:>4}×{height:<4} {machines:>3} {cells:>10} {steps:>10.0} {sim:>7.2} {raster_full:>7.2} {raster_dirty:>7.2} {clone_full:>7.2} {clone_dirty:>7.2} {total_full:>8.2} {total_dirty:>8.2}"
    );
}

fn header() {
    println!(
        "{:<28} {:>9} {:>3} {:>10} {:>10} {:>7} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8}",
        "name",
        "canvas",
        "m",
        "cells",
        "steps",
        "sim",
        "fullcp",
        "dirtycp",
        "fullcln",
        "dirtycln",
        "fulltot",
        "dirtytot"
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
