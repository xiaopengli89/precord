use crate::opt::Opts;
use crate::types::{CpuInfo, GpuInfo};
use clap::Parser;
use precord_core::{Features, Pid, System};
use std::thread;
use std::time::{Duration, Instant};

mod consumer_csv;
mod consumer_json;
mod consumer_svg;
mod opt;
mod types;
mod utils;

fn main() {
    let mut opts: Opts = Opts::parse();
    let sys_category = utils::drain_filter_vec(&mut opts.category, |c| c.starts_with("sys_"));

    let mut timestamps = vec![];

    let (processes, cpu_info, cpu_frequency_max, gpu_info) = {
        let mut features = Features::PROCESS;

        if opts.category.contains(&"gpu".to_owned())
            || sys_category.contains(&"sys_gpu".to_string())
        {
            features.insert(Features::GPU);
        }
        if opts.category.contains(&"fps".to_string()) {
            features.insert(Features::FPS);
        }
        if sys_category.contains(&"sys_cpu_freq".to_string()) {
            features.insert(Features::CPU_FREQUENCY);
        }

        let mut system = System::new(features, []);
        system.update();

        let mut processes = opts.find_processes(&system);

        let mut system = System::new(features, processes.iter().map(|p| p.pid));
        system.update();

        let mut cpu_info: Vec<CpuInfo> = vec![];
        let mut cpu_frequency_max: f32 = 1000.0;
        let mut gpu_info: Vec<GpuInfo> = vec![];

        let mut last_record_time = Instant::now();

        for i in 0..opts.times {
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

                for (idx, c) in opts.category.iter().enumerate() {
                    match c.as_str() {
                        "cpu" => {
                            if let Some(cpu_percent) = system.process_cpu_utilization(process.pid) {
                                process.value_percents[idx].push(cpu_percent);

                                message.push_str(&format!(" / CPU {:.2}%", cpu_percent));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        "mem" => {
                            if let Some(mem_usage) = system.process_mem(process.pid) {
                                let mem_usage = mem_usage / 1024.0;
                                process.value_percents[idx].push(mem_usage);

                                message.push_str(&format!(" / MEM {:.2}M", mem_usage));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        "gpu" => {
                            if let Some(gpu_percent) = system.process_gpu_percent(process.pid) {
                                process.value_percents[idx].push(gpu_percent);

                                message.push_str(&format!(" / GPU {:.2}%", gpu_percent));
                            } else {
                                process.valid = false;
                                continue 'p;
                            }
                        }
                        "fps" => {
                            let fps = system.process_fps(process.pid);
                            process.value_percents[idx].push(fps);

                            message.push_str(&format!(" / FPS {}", fps));
                        }
                        _ => unimplemented!(),
                    }
                }

                println!("{}", message);
            }

            // System
            for c in sys_category.iter() {
                match c.as_str() {
                    "sys_cpu_freq" => {
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
                    "sys_gpu" => {
                        let sys_gpu_utilization = system.system_gpu_percent().unwrap();

                        println!("System GPU Utilization: {}%", sys_gpu_utilization);

                        if gpu_info.is_empty() {
                            gpu_info.push(GpuInfo {
                                utilization: vec![sys_gpu_utilization],
                            });
                        } else {
                            gpu_info[0].utilization.push(sys_gpu_utilization);
                        }
                    }
                    _ => unreachable!(),
                }
            }

            println!("================ {}/{}", i, opts.times);

            let _ = utils::drain_filter_vec(&mut processes, |p| !p.valid);

            timestamps.push(chrono::Local::now());
        }

        (processes, cpu_info, cpu_frequency_max, gpu_info)
    };

    for output in opts.output.iter() {
        if let Some(ext) = output.extension() {
            if ext == "csv" {
                consumer_csv::consume(
                    output,
                    &opts.category,
                    &sys_category,
                    &timestamps,
                    &processes,
                    &cpu_info,
                    &gpu_info,
                );
            } else if ext == "svg" {
                consumer_svg::consume(
                    output,
                    &opts,
                    &sys_category,
                    &processes,
                    &cpu_info,
                    cpu_frequency_max,
                    &gpu_info,
                );
            } else if ext == "json" {
                consumer_json::consume(
                    output,
                    &opts.category,
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
