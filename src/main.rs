use crate::opt::{Opts, ProcessCategory, SystemCategory};
use crate::types::{ProcessInfo, SystemMetrics};
use clap::Parser;
use crossterm::style::Stylize;
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

    if let Some(action) = opts.action {
        for i in 0..2 {
            match action.exec() {
                Ok(_) => break,
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
        return;
    }

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

    let mut features = Features::PROCESS;

    if proc_category.contains(&ProcessCategory::Gpu)
        || proc_category.contains(&ProcessCategory::Vram)
        || sys_category.contains(&SystemCategory::Gpu)
    {
        features.insert(Features::GPU);
    }
    if proc_category.contains(&ProcessCategory::Fps) {
        features.insert(Features::FPS);
    }
    if proc_category.contains(&ProcessCategory::NetIn)
        || proc_category.contains(&ProcessCategory::NetOut)
    {
        features.insert(Features::NET_TRAFFIC);
    }
    if proc_category.contains(&ProcessCategory::Kobject) {
        features.insert(Features::K_OBJECT);
    }
    if sys_category.contains(&SystemCategory::CpuFreq) {
        features.insert(Features::CPU_FREQUENCY);
    }
    if sys_category.contains(&SystemCategory::CpuTemp)
        || sys_category.contains(&SystemCategory::Power)
    {
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

    let mut system_metrics = vec![SystemMetrics::default(); sys_category.len()];

    let mut last_record_time = Instant::now();

    let outputs = utils::extend_path(&path_re, opts.output);
    if !utils::check_permission(&outputs) {
        println!("Permission denied");
        return;
    }

    let write_result = |proc_categories: &[ProcessCategory],
                        sys_categories: &[SystemCategory],
                        timestamps: &[chrono::DateTime<chrono::Local>],
                        processes: &[ProcessInfo],
                        system_metrics: &[SystemMetrics],
                        o: &[PathBuf]| {
        for output in o.into_iter() {
            if let Some(parent) = output.parent() {
                if parent.components().count() > 0 && !parent.exists() {
                    fs::create_dir_all(parent).unwrap();
                }
            }

            if let Some(ext) = output.extension() {
                let swp_file = output.with_extension(utils::SWP_EXTENSION);
                let mut valid = false;

                if ext == "csv" {
                    consumer_csv::consume(
                        &swp_file,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        system_metrics,
                    );
                    valid = true;
                } else if ext == "svg" {
                    consumer_svg::consume(
                        &swp_file,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        system_metrics,
                    );
                    valid = true;
                } else if ext == "json" {
                    consumer_json::consume(
                        &swp_file,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        system_metrics,
                    );
                    valid = true;
                } else if ext == "html" {
                    consumer_html::consume(
                        &swp_file,
                        proc_categories,
                        sys_categories,
                        timestamps,
                        processes,
                        system_metrics,
                    );
                    valid = true;
                }

                if valid {
                    fs::rename(swp_file, output).unwrap();
                    println!("Write to {}\r", output.display());
                }
            }
        }
    };

    let mut prompt = None;

    if opts.interactive {
        prompt = utils::CommandPrompt::new();
    }

    if let Some(prompt) = &mut prompt {
        if !utils::overwrite_detect(&outputs, prompt) {
            return;
        }
    }

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
                    utils::Command::Timeout => {
                        break;
                    }
                    utils::Command::Continue => command_mode = false,
                    utils::Command::Write(p) => {
                        let p = utils::extend_path(&path_re, p);
                        let p = if !p.is_empty() { &p } else { &outputs };
                        if utils::check_permission(p) {
                            write_result(
                                &proc_category,
                                &sys_category,
                                &timestamps,
                                &processes,
                                &system_metrics,
                                p,
                            );
                        } else {
                            println!("Permission denied\r");
                        }
                        command_mode = true;
                    }
                    utils::Command::Quit => return,
                    utils::Command::WriteThenQuit(p) => {
                        let p = utils::extend_path(&path_re, p);
                        let p = if !p.is_empty() { &p } else { &outputs };
                        if utils::check_permission(p) {
                            write_result(
                                &proc_category,
                                &sys_category,
                                &timestamps,
                                &processes,
                                &system_metrics,
                                p,
                            );
                            return;
                        } else {
                            println!("Permission denied\r");
                            command_mode = true;
                        }
                    }
                    utils::Command::Time(d) => {
                        end_time = Some(chrono::Local::now() + d);
                        opts.count = None;
                    }
                    utils::Command::Yes
                    | utils::Command::No
                    | utils::Command::Empty
                    | utils::Command::Unknown => command_mode = true,
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
        } else if let Some(auto_save) = opts.auto_save {
            if i as u64 % auto_save == 0 {
                write_result(
                    &proc_category,
                    &sys_category,
                    &timestamps,
                    &processes,
                    &system_metrics,
                    &outputs,
                );
            }
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
            let rows = c.sample(&mut system, opts.gpu_calc);

            println!(
                "{:?}: [{}]\r",
                c,
                rows.iter()
                    .map(|f| format!("{:.2}{}", f, c.unit()).with(c.color()).to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            let mut metrics = &mut system_metrics[idx];

            if metrics.rows.is_empty() {
                metrics.rows = rows.into_iter().map(|row| vec![row]).collect();
            } else {
                for (row, v) in metrics.rows.iter_mut().zip(rows.into_iter()) {
                    row.push(v);
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
        &system_metrics,
        &outputs,
    );
}
