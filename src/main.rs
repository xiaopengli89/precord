use crate::opt::{Category, Opts, ProcessCategory, SystemCategory};
use crate::types::{CpuInfo, GpuInfo};
use clap::Parser;
use precord_core::{Features, Pid, System};
use regex::Regex;
use std::time::{Duration, Instant};
use std::{fs, thread};

mod consumer_csv;
mod consumer_json;
mod consumer_svg;
mod opt;
mod types;
mod utils;

fn main() {
    let path_re = Regex::new(r"^\{([\w,]+)}$").unwrap();
    let opts: Opts = Opts::parse();
    let proc_category: Vec<_> = opts
        .category
        .iter()
        .filter_map(|&c| match c {
            Category::CPU => Some(ProcessCategory::CPU),
            Category::Mem => Some(ProcessCategory::Mem),
            Category::GPU => Some(ProcessCategory::GPU),
            Category::FPS => Some(ProcessCategory::FPS),
            _ => None,
        })
        .collect();
    let sys_category: Vec<_> = opts
        .category
        .iter()
        .flat_map(|&c| match c {
            Category::SysCPUFreq => Some(SystemCategory::CPUFreq),
            Category::SysGPU => Some(SystemCategory::GPU),
            _ => None,
        })
        .collect();

    let mut timestamps = vec![];

    let (processes, cpu_info, cpu_frequency_max, gpu_info) = {
        let mut features = Features::PROCESS;

        if proc_category.contains(&ProcessCategory::GPU)
            || sys_category.contains(&SystemCategory::GPU)
        {
            features.insert(Features::GPU);
        }
        if proc_category.contains(&ProcessCategory::FPS) {
            features.insert(Features::FPS);
        }
        if sys_category.contains(&SystemCategory::CPUFreq) {
            features.insert(Features::CPU_FREQUENCY);
        }

        let mut system = System::new(Features::PROCESS, []);
        system.update();

        let mut processes = opts.find_processes(&system, proc_category.len());

        let mut system = System::new(features, processes.iter().map(|p| p.pid));
        system.update();

        let mut cpu_info: Vec<CpuInfo> = vec![];
        let mut cpu_frequency_max: f32 = 1000.0;
        let mut gpu_info: Vec<GpuInfo> = vec![];

        let mut last_record_time = Instant::now();

        for i in 0..opts.count {
            let now = Instant::now();
            let since = now.saturating_duration_since(last_record_time);
            let delay = Duration::from_secs(opts.interval).saturating_sub(since);
            if !delay.is_zero() {
                thread::sleep(delay);
                last_record_time = Instant::now();
            } else {
                last_record_time = now;
            }

            system.update();

            // Process
            'p: for process in processes.iter_mut() {
                let mut message = format!("{}({})", &process.name, process.pid);

                for (idx, &c) in proc_category.iter().enumerate() {
                    match c {
                        ProcessCategory::CPU => {
                            if let Some(cpu_usage) = system.process_cpu_usage(process.pid) {
                                process.values[idx].push(cpu_usage);

                                message.push_str(&format!(" / CPU {:.2}%", cpu_usage));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        ProcessCategory::Mem => {
                            if let Some(mem_usage) = system.process_mem(process.pid) {
                                let mem_usage = mem_usage / 1024.0;
                                process.values[idx].push(mem_usage);

                                message.push_str(&format!(" / MEM {:.2}M", mem_usage));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        ProcessCategory::GPU => {
                            if let Some(gpu_usage) = system.process_gpu_usage(process.pid) {
                                process.values[idx].push(gpu_usage);

                                message.push_str(&format!(" / GPU {:.2}%", gpu_usage));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        ProcessCategory::FPS => {
                            let fps = system.process_fps(process.pid);
                            process.values[idx].push(fps);

                            message.push_str(&format!(" / FPS {}", fps));
                        }
                    }
                }

                println!("{}", message);
            }

            // System
            for &c in sys_category.iter() {
                match c {
                    SystemCategory::CPUFreq => {
                        let cpu_frequency = system.cpu_frequency();

                        println!(
                            "CPU Frequency: [{}]",
                            cpu_frequency
                                .iter()
                                .map(|f| format!("{}MHz", f))
                                .collect::<Vec<String>>()
                                .join(", ")
                        );

                        if cpu_info.is_empty() {
                            cpu_info = cpu_frequency
                                .into_iter()
                                .map(|f| {
                                    cpu_frequency_max = cpu_frequency_max.max(f);
                                    CpuInfo { freq: vec![f] }
                                })
                                .collect();
                        } else {
                            for (sum, f) in cpu_info.iter_mut().zip(cpu_frequency.into_iter()) {
                                cpu_frequency_max = cpu_frequency_max.max(f);
                                sum.freq.push(f);
                            }
                        }
                    }
                    SystemCategory::GPU => {
                        let sys_gpu_usage = system.system_gpu_usage().unwrap();

                        println!("System GPU Usage: {}%", sys_gpu_usage);

                        if gpu_info.is_empty() {
                            gpu_info.push(GpuInfo {
                                usage: vec![sys_gpu_usage],
                            });
                        } else {
                            gpu_info[0].usage.push(sys_gpu_usage);
                        }
                    }
                }
            }

            println!("================ {}/{}", i, opts.count);

            let _ = utils::drain_filter_vec(&mut processes, |p| !p.valid);

            timestamps.push(chrono::Local::now());
        }

        (processes, cpu_info, cpu_frequency_max, gpu_info)
    };

    let outputs = opts
        .output
        .into_iter()
        .filter_map(|path| {
            let ext = path.extension()?;
            let mut paths = vec![];

            if let Some(ext) = ext.to_str() {
                if path_re.is_match(ext) {
                    for cap in path_re.captures_iter(ext) {
                        for ext_str in cap[1].split(',') {
                            let mut path_cloned = path.clone();
                            path_cloned.set_extension(ext_str);
                            paths.push(path_cloned);
                        }
                    }
                } else {
                    paths.push(path);
                }
            } else {
                paths.push(path);
            };

            Some(paths)
        })
        .flatten();

    for output in outputs {
        if let Some(parent) = output.parent() {
            if parent.components().count() > 0 && !parent.exists() {
                fs::create_dir_all(parent).unwrap();
            }
        }

        if let Some(ext) = output.extension() {
            if ext == "csv" {
                consumer_csv::consume(
                    output,
                    &proc_category,
                    &sys_category,
                    &timestamps,
                    &processes,
                    &cpu_info,
                    &gpu_info,
                );
            } else if ext == "svg" {
                consumer_svg::consume(
                    output,
                    &proc_category,
                    &sys_category,
                    &timestamps,
                    &processes,
                    &cpu_info,
                    cpu_frequency_max,
                    &gpu_info,
                );
            } else if ext == "json" {
                consumer_json::consume(
                    output,
                    &proc_category,
                    &sys_category,
                    &timestamps,
                    &processes,
                    &cpu_info,
                    &gpu_info,
                );
            }
        }
    }
}
