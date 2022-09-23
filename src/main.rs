use crate::opt::{Opts, ProcessCategory, SystemCategory};
use crate::types::{CpuInfo, GpuInfo, PhysicalCpuInfo, ProcessInfo};
use crate::utils::{extend_path, Command, CommandPrompt};
use clap::Parser;
use crossterm::style::{Color, Stylize};
use precord_core::{Error, Features, Pid, System};
use regex::Regex;
use std::fmt::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{fs, thread};

mod consumer_csv;
mod consumer_html;
mod consumer_json;
mod consumer_svg;
mod opt;
mod types;
mod utils;

fn main() {
    let path_re = Regex::new(r"^\{([\w,]+)}$").unwrap();
    let mut opts: Opts = Opts::parse();

    if opts.count.is_none() && opts.time.is_none() {
        opts.count = Some(30);
    }

    let proc_category: Vec<_> = opts
        .category
        .iter()
        .filter_map(|&c| c.to_process())
        .collect();
    let sys_category: Vec<_> = opts.category.iter().flat_map(|&c| c.to_system()).collect();

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
    if proc_category.contains(&ProcessCategory::KObject) {
        features.insert(Features::K_OBJECT);
    }
    if sys_category.contains(&SystemCategory::CPUFreq) {
        features.insert(Features::CPU_FREQUENCY);
    }
    if sys_category.contains(&SystemCategory::CPUTemp) {
        features.insert(Features::SMC);
    }

    let system = System::new(Features::PROCESS, []).unwrap();

    let mut processes = opts.find_processes(&system, proc_category.len());

    if (processes.is_empty() || proc_category.is_empty()) && sys_category.is_empty() {
        println!("No tasks available");
        return;
    }

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

    let mut prompt = CommandPrompt::new();

    if let Some(prompt) = &mut prompt {
        if !utils::overwrite_detect(&outputs, prompt) {
            return;
        }
    }

    let terminal_colors = [
        Color::DarkGreen,
        Color::DarkCyan,
        Color::AnsiValue(208),
        Color::DarkYellow,
        Color::DarkBlue,
        Color::DarkMagenta,
    ];
    let mut end_time = None;

    for i in -(opts.skip as isize).. {
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

            if let Some(prompt) = &mut prompt {
                match prompt.command(delay) {
                    Command::Timeout => {
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
                    Command::Yes | Command::No | Command::Empty | Command::Unknown => {
                        command_mode = true
                    }
                }
            } else if let Some(delay) = delay {
                thread::sleep(delay);
                break;
            } else {
                unreachable!();
            }
        }

        last_record_time = Instant::now();

        system.update(last_record_time);

        if i < 0 {
            continue;
        }

        if i == 0 {
            end_time = opts
                .time
                .map(|d| chrono::Local::now() + chrono::Duration::from_std(*d).unwrap());
        }

        // Process
        if !proc_category.is_empty() {
            for process in processes.iter_mut() {
                let mut message = format!("{}({})", &process.name, process.pid);

                for (idx, &c) in proc_category.iter().enumerate() {
                    if let Some(v) = c.sample(&mut system, opts.gpu_calc, process.pid) {
                        process.valid = true;
                        process.values[idx].push(v);
                        message.push_str(&format!(
                            " / {}",
                            format!("{:?} {:.2}{}", c, v, c.unit()).with(c.color())
                        ));
                    } else {
                        process.valid = false;
                        process.values[idx].push(0.0);
                        message.push_str(&format!(" / {}", format!("{:?} Lost", c).dark_red()));
                    }
                }

                println!("{}\r", message);
            }
        }

        // System
        for (idx, &c) in sys_category.iter().enumerate() {
            let color = terminal_colors[idx];
            match c {
                SystemCategory::CPUFreq => {
                    let cpus_frequency = system.cpus_frequency().unwrap();

                    println!(
                        "CPUs Frequency: [{}]\r",
                        cpus_frequency
                            .iter()
                            .map(|f| format!("{}MHz", f).with(color).to_string())
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
                            .map(|f| format!("{}°C", f).with(color).to_string())
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
                    let sys_gpu_usage =
                        system.system_gpu_usage(opts.gpu_calc.into()).unwrap_or(0.0);

                    println!(
                        "System GPU Usage: {}\r",
                        format!("{}%", sys_gpu_usage).with(color)
                    );

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

        let mut progress = format!("================ {}", i + 1);

        if let Some(count) = opts.count {
            let _ = write!(&mut progress, " / {}", count);
        }

        if let Some(end_time) = end_time {
            let _ = write!(&mut progress, " / {}", end_time);
        }

        println!("{}\r", progress);

        // let _ = utils::drain_filter_vec(&mut processes, |p| !p.valid);

        let now = chrono::Local::now();
        timestamps.push(now);

        if let Some(count) = opts.count {
            if i + 1 >= count as isize {
                break;
            }
        }

        if let Some(end_time) = end_time {
            if now > end_time {
                break;
            }
        }
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
