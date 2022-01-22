use crate::opt::{Category, Opts, ProcessCategory, SystemCategory};
use crate::types::{CpuInfo, GpuInfo, PhysicalCpuInfo, ProcessInfo};
use crate::utils::{extend_path, Command, CommandPrompt};
use clap::Parser;
use precord_core::{Error, Features, Pid, System};
use regex::Regex;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
// #[cfg(target_os = "windows")]
// use windows::Win32::UI::Shell::ShellExecuteW;

mod consumer_csv;
mod consumer_html;
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
            Category::NetIn => Some(ProcessCategory::NetIn),
            Category::NetOut => Some(ProcessCategory::NetOut),
            _ => None,
        })
        .collect();
    let sys_category: Vec<_> = opts
        .category
        .iter()
        .flat_map(|&c| match c {
            Category::SysCPUFreq => Some(SystemCategory::CPUFreq),
            Category::SysCPUTemp => Some(SystemCategory::CPUTemp),
            Category::SysGPU => Some(SystemCategory::GPU),
            _ => None,
        })
        .collect();

    let mut prompt = CommandPrompt::new();

    let mut timestamps = vec![];
    let mut cpu_frequency_max: f32 = 1000.0;
    let mut cpu_temperature_max: f32 = 100.0;
    let mut physical_cpu_info: Vec<PhysicalCpuInfo> = vec![];

    let mut features = Features::PROCESS;

    if proc_category.contains(&ProcessCategory::GPU) || sys_category.contains(&SystemCategory::GPU)
    {
        features.insert(Features::GPU);
    }
    if proc_category.contains(&ProcessCategory::FPS) {
        features.insert(Features::FPS);
    }
    if proc_category.contains(&ProcessCategory::NetIn)
        || proc_category.contains(&ProcessCategory::NetOut)
    {
        features.insert(Features::NET_TRAFFIC);
    }
    if sys_category.contains(&SystemCategory::CPUFreq) {
        features.insert(Features::CPU_FREQUENCY);
    }
    if sys_category.contains(&SystemCategory::CPUTemp) {
        features.insert(Features::SMC);
    }

    let mut system = System::new(Features::PROCESS, []).unwrap();
    system.update();

    let mut processes = opts.find_processes(&system, proc_category.len());

    let mut system = None;
    for i in 0..2 {
        match System::new(features, processes.iter().map(|p| p.pid)) {
            Ok(system1) => {
                system = Some(system1);
                break;
            }
            Err(Error::AccessDenied) => {
                if i == 0 {
                    utils::adjust_privileges();
                } else {
                    panic!("{:?}", Error::AccessDenied);
                }
            }
            Err(err) => panic!("{:?}", err),
        }
    }
    let mut system = system.unwrap();

    let mut cpu_info: Vec<CpuInfo> = vec![];
    let mut gpu_info: Vec<GpuInfo> = vec![];

    let mut last_record_time = Instant::now();

    let outputs = extend_path(&path_re, opts.output);

    let write_result = |proc_categories: &[ProcessCategory],
                        sys_categories: &[SystemCategory],
                        timestamps: &[chrono::DateTime<chrono::Local>],
                        processes: &[ProcessInfo],
                        cpu_info: &[CpuInfo],
                        cpu_frequency_max: f32,
                        physical_cpu_info: &[PhysicalCpuInfo],
                        cpu_temperature_max: f32,
                        gpu_info: &[GpuInfo],
                        o: &[PathBuf]| {
        for output in o.into_iter() {
            if let Some(parent) = output.parent() {
                if parent.components().count() > 0 && !parent.exists() {
                    fs::create_dir_all(parent).unwrap();
                }
            }

            if let Some(ext) = output.extension() {
                if ext == "csv" {
                    println!("Write to {}\r", output.display());
                    consumer_csv::consume(
                        output,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        cpu_info,
                        physical_cpu_info,
                        gpu_info,
                    );
                } else if ext == "svg" {
                    println!("Write to {}\r", output.display());
                    consumer_svg::consume(
                        output,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        cpu_info,
                        cpu_frequency_max,
                        physical_cpu_info,
                        cpu_temperature_max,
                        gpu_info,
                    );
                } else if ext == "json" {
                    println!("Write to {}\r", output.display());
                    consumer_json::consume(
                        output,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        cpu_info,
                        physical_cpu_info,
                        gpu_info,
                    );
                } else if ext == "html" {
                    println!("Write to {}\r", output.display());
                    consumer_html::consume(
                        output,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        cpu_info,
                        cpu_frequency_max,
                        physical_cpu_info,
                        cpu_temperature_max,
                        gpu_info,
                    );
                }
            }
        }
    };

    for i in 0..opts.count {
        let mut command_mode = false;

        loop {
            let delay = if command_mode {
                None
            } else {
                let now = Instant::now();
                let since = now.saturating_duration_since(last_record_time);
                let delay = Duration::from_secs(opts.interval).saturating_sub(since);
                Some(delay)
            };

            match prompt.command(delay) {
                Command::Timeout => {
                    last_record_time = Instant::now();
                    break;
                }
                Command::Continue => command_mode = false,
                Command::Write(p) => {
                    let p = extend_path(&path_re, p);
                    write_result(
                        &proc_category,
                        &sys_category,
                        &timestamps,
                        &processes,
                        &cpu_info,
                        cpu_frequency_max,
                        &physical_cpu_info,
                        cpu_temperature_max,
                        &gpu_info,
                        if !p.is_empty() { &p } else { &outputs },
                    );
                    command_mode = true;
                }
                Command::Quit => return,
                Command::WriteThenQuit(p) => {
                    let p = extend_path(&path_re, p);
                    write_result(
                        &proc_category,
                        &sys_category,
                        &timestamps,
                        &processes,
                        &cpu_info,
                        cpu_frequency_max,
                        &physical_cpu_info,
                        cpu_temperature_max,
                        &gpu_info,
                        if !p.is_empty() { &p } else { &outputs },
                    );
                    return;
                }
                Command::Unknown => command_mode = true,
            }
        }

        system.update();

        // Process
        for process in processes.iter_mut() {
            let mut message = format!("{}({})", &process.name, process.pid);

            for (idx, &c) in proc_category.iter().enumerate() {
                match c {
                    ProcessCategory::CPU => {
                        if let Some(cpu_usage) = system.process_cpu_usage(process.pid) {
                            process.valid = true;
                            process.values[idx].push(cpu_usage);
                            message.push_str(&format!(" / CPU {:.2}%", cpu_usage));
                        } else {
                            process.valid = false;
                            process.values[idx].push(0.0);
                            message.push_str(" / CPU Lost");
                        }
                    }
                    ProcessCategory::Mem => {
                        if let Some(mem_usage) = system.process_mem(process.pid) {
                            let mem_usage = mem_usage / 1024.0;

                            process.valid = true;
                            process.values[idx].push(mem_usage);
                            message.push_str(&format!(" / MEM {:.2}M", mem_usage));
                        } else {
                            process.valid = false;
                            process.values[idx].push(0.0);
                            message.push_str(" / MEM Lost");
                        }
                    }
                    ProcessCategory::GPU => {
                        if let Some(gpu_usage) = system.process_gpu_usage(process.pid) {
                            process.valid = true;
                            process.values[idx].push(gpu_usage);
                            message.push_str(&format!(" / GPU {:.2}%", gpu_usage));
                        } else {
                            process.valid = false;
                            process.values[idx].push(0.0);
                            message.push_str(" / GPU Lost");
                        }
                    }
                    ProcessCategory::FPS => {
                        let fps = system.process_fps(process.pid);
                        process.values[idx].push(fps);

                        message.push_str(&format!(" / FPS {}", fps));
                    }
                    ProcessCategory::NetIn => {
                        if let Some(net_in) = system.process_net_traffic_in(process.pid) {
                            let net_in = (net_in >> 10) as f32;
                            process.valid = true;
                            process.values[idx].push(net_in);
                            message.push_str(&format!(" / NET_IN {:.2}KBps", net_in));
                        } else {
                            process.valid = false;
                            process.values[idx].push(0.0);
                            message.push_str(" / NET_IN Lost");
                        }
                    }
                    ProcessCategory::NetOut => {
                        if let Some(net_out) = system.process_net_traffic_out(process.pid) {
                            let net_out = (net_out >> 10) as f32;
                            process.valid = true;
                            process.values[idx].push(net_out);
                            message.push_str(&format!(" / NET_OUT {:.2}KBps", net_out));
                        } else {
                            process.valid = false;
                            process.values[idx].push(0.0);
                            message.push_str(" / NET_OUT Lost");
                        }
                    }
                }
            }

            println!("{}\r", message);
        }

        // System
        for &c in sys_category.iter() {
            match c {
                SystemCategory::CPUFreq => {
                    let cpus_frequency = system.cpus_frequency().unwrap();

                    println!(
                        "CPUs Frequency: [{}]\r",
                        cpus_frequency
                            .iter()
                            .map(|f| format!("{}MHz", f))
                            .collect::<Vec<String>>()
                            .join(", ")
                    );

                    if cpu_info.is_empty() {
                        cpu_info = cpus_frequency
                            .into_iter()
                            .map(|f| {
                                cpu_frequency_max = cpu_frequency_max.max(f);
                                CpuInfo { freq: vec![f] }
                            })
                            .collect();
                    } else {
                        for (sum, f) in cpu_info.iter_mut().zip(cpus_frequency.into_iter()) {
                            cpu_frequency_max = cpu_frequency_max.max(f);
                            sum.freq.push(f);
                        }
                    }
                }
                SystemCategory::CPUTemp => {
                    let cpus_temperature = system.cpus_temperature().unwrap();

                    println!(
                        "CPUs Temperature: [{}]\r",
                        cpus_temperature
                            .iter()
                            .map(|f| format!("{}°C", f))
                            .collect::<Vec<String>>()
                            .join(", ")
                    );

                    if physical_cpu_info.is_empty() {
                        physical_cpu_info = cpus_temperature
                            .into_iter()
                            .map(|f| {
                                cpu_temperature_max = cpu_temperature_max.max(f);
                                PhysicalCpuInfo { temp: vec![f] }
                            })
                            .collect();
                    } else {
                        for (sum, f) in physical_cpu_info
                            .iter_mut()
                            .zip(cpus_temperature.into_iter())
                        {
                            cpu_temperature_max = cpu_temperature_max.max(f);
                            sum.temp.push(f);
                        }
                    }
                }
                SystemCategory::GPU => {
                    let sys_gpu_usage = system.system_gpu_usage().unwrap();

                    println!("System GPU Usage: {}%\r", sys_gpu_usage);

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

        println!("================ {}/{}\r", i, opts.count);

        // let _ = utils::drain_filter_vec(&mut processes, |p| !p.valid);

        timestamps.push(chrono::Local::now());
    }

    write_result(
        &proc_category,
        &sys_category,
        &timestamps,
        &processes,
        &cpu_info,
        cpu_frequency_max,
        &physical_cpu_info,
        cpu_temperature_max,
        &gpu_info,
        &outputs,
    );
}
