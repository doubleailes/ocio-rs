// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Philippe Llerena.

use super::Runner;
use ocio::CpuProcessor;
use std::{hint::black_box, time::Instant};

pub fn arguments() -> Option<usize> {
    let mut bench = false;
    let mut iterations = 5;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => bench = true,
            "--iterations" => {
                iterations = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .filter(|&n| n > 0)
                    .expect("--iterations needs a positive integer");
            }
            "--help" | "-h" => {
                println!("Usage: ocio-wgpu-example [--bench] [--iterations N]\nAlways validates CPU/GPU agreement; --bench also times 720p, 1080p and 4K frames.");
                std::process::exit(0);
            }
            _ => panic!("Unknown argument: {arg}"),
        }
    }
    bench.then_some(iterations)
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    (samples[(n - 1) / 2] + samples[n / 2]) / 2.0
}

pub fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runner: &Runner,
    uniforms: &[u8],
    cpu: &CpuProcessor,
    iterations: usize,
) {
    println!(
        "\nBenchmark: release={}, {iterations} samples, 2 warmups, median wall time, exposure=0",
        !cfg!(debug_assertions)
    );
    println!("CPU: single-thread apply_pixels, input reset excluded.");
    println!("GPU resident: reused buffers/bind groups; submit + compute + completion wait, no transfer/readback.");
    println!("GPU round trip: allocation + packing + upload + compute + readback + unpacking.");
    println!("Shader compilation and LUT upload excluded from both GPU timings.");
    println!(
        "resolution,CPU ms,GPU resident ms,GPU round trip ms,resident speedup,round trip speedup"
    );
    let seeds = super::test_pixels();
    for (width, height) in [(1280, 720), (1920, 1080), (3840, 2160)] {
        let count = width * height;
        let size = (count * 16) as u64;
        if size > device.limits().max_storage_buffer_binding_size
            || size > device.limits().max_buffer_size
        {
            println!("{width}x{height}: skipped (device buffer limits)");
            continue;
        }
        let pixels: Vec<_> = seeds.iter().copied().cycle().take(count).collect();
        let mut work = pixels.clone();
        let job = runner.prepare(device, queue, uniforms, &pixels);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("upload completion");
        let mut cpu_times = Vec::new();
        let mut resident_times = Vec::new();
        let mut round_trip_times = Vec::new();
        for i in 0..iterations + 2 {
            work.copy_from_slice(&pixels);
            let start = Instant::now();
            cpu.apply_pixels(black_box(&mut work));
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
            black_box(&work);

            let start = Instant::now();
            job.submit(runner, device, queue, false);
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("compute completion");
            let resident_ms = start.elapsed().as_secs_f64() * 1000.0;

            let start = Instant::now();
            let out = runner.run(device, queue, uniforms, &pixels);
            let round_trip_ms = start.elapsed().as_secs_f64() * 1000.0;
            black_box(&out);
            // Check the actual benchmark frame too, outside the measured regions.
            if i == 0 {
                let quantize = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as i32;
                let mut worst = 0;
                for (c, g) in work.iter().flatten().zip(out.iter().flatten()) {
                    assert!(
                        c.is_finite() && g.is_finite(),
                        "non-finite benchmark result"
                    );
                    worst = worst.max(quantize(*c).abs_diff(quantize(*g)));
                }
                assert!(worst <= 2, "benchmark frame differs by {worst} code values");
            }
            if i >= 2 {
                cpu_times.push(cpu_ms);
                resident_times.push(resident_ms);
                round_trip_times.push(round_trip_ms);
            }
        }
        let c = median(&mut cpu_times);
        let g = median(&mut resident_times);
        let r = median(&mut round_trip_times);
        println!(
            "{width}x{height},{c:.3},{g:.3},{r:.3},{:.2}x,{:.2}x",
            c / g,
            c / r
        );
    }
}
