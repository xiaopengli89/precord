#![feature(drain_filter)]

use crate::types::GpuInfo;
use clap::{AppSettings, Clap};
use futures::stream::StreamExt;
use heim::process::{CpuUsage, Pid, Process, Status};
use heim::units::ratio;
use precord_core::{Features, System};
use std::time::{Duration, Instant};
use sysinfo::{ProcessExt, RefreshKind, SystemExt};

mod consumer_csv;
mod consumer_json;
mod consumer_svg;
mod types;

fn main() {
    let mut opts: Opts = Opts::parse();
    let sys_category: Vec<String> = opts
        .category
        .drain_filter(|c| c.starts_with("sys_"))
        .collect();

    let mut timestamps = vec![];

    let (processes, cpu_info, cpu_frequency_max, gpu_info) = futures::executor::block_on(async {
        let mut features = Features::empty();

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

        let mut sysinfo_system =
            sysinfo::System::new_with_specifics(RefreshKind::new().with_processes());
        sysinfo_system.refresh_processes();

        let mut processes = opts.find_processes(&sysinfo_system).await;
        let mut system = System::new(features, processes.iter().map(|p| p.process.pid()));
        let mut cpu_info: Vec<CpuInfo> = vec![];
        let mut cpu_frequency_max: f32 = 1000.0;
        let mut gpu_info: Vec<GpuInfo> = vec![];

        let mut last_record_time = Instant::now();

        for i in 0..opts.times {
            let now = Instant::now();
            let since = now.saturating_duration_since(last_record_time);
            let delay = Duration::from_secs(opts.interval).saturating_sub(since);
            if !delay.is_zero() {
                futures_timer::Delay::new(delay).await;
                last_record_time = Instant::now();
            } else {
                last_record_time = now;
            }

            system.update();

            // Process
            'p: for process in processes.iter_mut() {
                let mut message = format!("{}({})", &process.name, process.process.pid(),);

                for (idx, c) in opts.category.iter().enumerate() {
                    match c.as_str() {
                        "cpu" => {
                            if let Some(cpu_percent) = process.poll_cpu_percent().await {
                                process.value_percents[idx].push(cpu_percent);

                                message.push_str(&format!(" / CPU {:.2}%", cpu_percent));
                            } else {
                                continue 'p;
                            }
                        }
                        "mem" => {
                            if let Some(mem_usage) = process.poll_mem_usage().await {
                                process.value_percents[idx].push(mem_usage);

                                message.push_str(&format!(" / MEM {:.2}M", mem_usage));
                            } else {
                                continue 'p;
                            }
                        }
                        "gpu" => {
                            if let Some(gpu_percent) = process.poll_gpu_percent(&mut system) {
                                process.value_percents[idx].push(gpu_percent);

                                message.push_str(&format!(" / GPU {:.2}%", gpu_percent));
                            } else {
                                continue 'p;
                            }
                        }
                        "fps" => {
                            let fps = system.process_fps(process.process.pid());
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

            processes.drain_filter(|p| !p.valid);

            timestamps.push(chrono::Local::now());
        }

        (processes, cpu_info, cpu_frequency_max, gpu_info)
    });

    for output in opts.output.iter() {
        if output.ends_with(".csv") {
            consumer_csv::consume(
                output,
                &opts.category,
                &sys_category,
                &timestamps,
                &processes,
                &cpu_info,
                &gpu_info,
            );
        } else if output.ends_with(".svg") {
            consumer_svg::consume(
                output,
                &opts,
                &sys_category,
                &processes,
                &cpu_info,
                cpu_frequency_max,
                &gpu_info,
            );
        } else if output.ends_with(".json") {
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

pub struct ProcessInfo {
    process: Process,
    name: String,
    command: String,
    value_percents: Vec<Vec<f32>>,
    prev_cpu_usage: CpuUsage,
    valid: bool,
}

impl ProcessInfo {
    async fn new(
        sysinfo_system: &sysinfo::System,
        process: Process,
        name: String,
        categories: &[String],
    ) -> Self {
        let prev_cpu_usage = process.cpu_usage().await.unwrap();
        let command = sysinfo_system
            .process(process.pid())
            .unwrap()
            .cmd()
            .join(" ");

        Self {
            process,
            name,
            command,
            value_percents: vec![vec![]; categories.len()],
            prev_cpu_usage,
            valid: true,
        }
    }

    async fn poll_cpu_percent(&mut self) -> Option<f32> {
        if let Ok(cpu_usage) = self.process.cpu_usage().await {
            let cpu_percent = (cpu_usage.clone()
                - std::mem::replace(&mut self.prev_cpu_usage, cpu_usage))
            .get::<ratio::percent>();

            Some(cpu_percent)
        } else {
            self.valid = false;
            None
        }
    }

    async fn poll_mem_usage(&mut self) -> Option<f32> {
        if let Ok(m) = self.process.memory().await {
            Some((m.rss().value as f64 / (1024.0 * 1024.0)) as _)
        } else {
            self.valid = false;
            None
        }
    }

    fn poll_gpu_percent(&mut self, system: &mut System) -> Option<f32> {
        let r = system.process_gpu_percent(self.process.pid());

        if r.is_none() {
            self.valid = false;
        }

        r
    }

    fn avg_percent(&self, idx: usize) -> f32 {
        if self.value_percents[idx].is_empty() {
            0.0
        } else {
            self.value_percents[idx].iter().sum::<f32>() / (self.value_percents[idx].len() as f32)
        }
    }
}

pub struct CpuInfo {
    freq: Vec<f32>,
}

impl CpuInfo {
    fn avg(&self) -> f32 {
        if self.freq.is_empty() {
            0.0
        } else {
            self.freq.iter().sum::<f32>() / (self.freq.len() as f32)
        }
    }
}

#[derive(Clap, Debug, Clone)]
#[clap(version = "0.3.1", author = "Xiaopeng Li <x.friday@outlook.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
pub struct Opts {
    #[clap(short, long)]
    process: Vec<Pid>,
    #[clap(long)]
    name: Vec<String>,
    #[clap(short, long)]
    output: Vec<String>,
    #[clap(short, long, default_value = "1")]
    interval: u64,
    #[clap(short, long, default_value = "30")]
    times: usize,
    #[clap(short, long, default_value = "cpu", possible_values = &["cpu", "mem", "gpu", "fps", "sys_cpu_freq", "sys_gpu"])]
    category: Vec<String>,
    #[clap(short, long)]
    recurse_children: bool,
}

impl Opts {
    async fn find_processes(&self, sysinfo_system: &sysinfo::System) -> Vec<ProcessInfo> {
        let mut processes: Vec<ProcessInfo> = vec![];

        if self.name.is_empty() {
            for &pid in self.process.iter() {
                if processes
                    .iter()
                    .position(|p| p.process.pid() == pid)
                    .is_some()
                {
                    continue;
                }

                let p = heim::process::get(pid).await.unwrap();
                let name = p.name().await.unwrap();
                processes.push(
                    ProcessInfo::new(sysinfo_system, p, name, self.category.as_slice()).await,
                );
            }
        } else {
            let mut all = Box::pin(heim::process::processes().await.unwrap());

            while let Some(p) = all.next().await {
                let p = match p {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                match p.status().await.unwrap() {
                    Status::Zombie => continue,
                    _ => {}
                }

                let name = p.name().await.unwrap();

                if self.process.contains(&p.pid()) {
                    processes.push(
                        ProcessInfo::new(sysinfo_system, p, name, self.category.as_slice()).await,
                    );
                } else {
                    for n in self.name.iter() {
                        if name.contains(n) {
                            processes.push(
                                ProcessInfo::new(sysinfo_system, p, name, self.category.as_slice())
                                    .await,
                            );
                            break;
                        }
                    }
                }
            }
        }

        if self.recurse_children {
            processes.extend(
                self.recurse_children(sysinfo_system, processes.as_slice())
                    .await,
            );
        }

        processes
    }

    async fn recurse_children(
        &self,
        sysinfo_system: &sysinfo::System,
        processes: &[ProcessInfo],
    ) -> Vec<ProcessInfo> {
        let mut all = Box::pin(heim::process::processes().await.unwrap());

        let recurse_parent = |mut parent: heim::process::Process| async move {
            if processes
                .iter()
                .position(|p| p.process.pid() == parent.pid())
                .is_some()
            {
                return true;
            }

            if parent.pid() == 0 {
                return false;
            }

            while let Ok(parent2) = parent.parent().await {
                if processes
                    .iter()
                    .position(|p| p.process.pid() == parent2.pid())
                    .is_some()
                {
                    return true;
                }
                parent = parent2;

                if parent.pid() == 0 {
                    return false;
                }
            }
            false
        };

        let mut children = vec![];

        while let Some(child) = all.next().await {
            let child = match child {
                Ok(child) => child,
                Err(_) => continue,
            };

            if child.pid() == 0 {
                continue;
            }

            if let Ok(status) = child.status().await {
                match status {
                    Status::Zombie => continue,
                    _ => {}
                }
            } else {
                continue;
            }

            if processes
                .iter()
                .position(|p| p.process.pid() == child.pid())
                .is_some()
            {
                continue;
            }

            if let Ok(parent) = child.parent().await {
                if recurse_parent(parent).await {
                    let name = child.name().await.unwrap();
                    children.push(
                        ProcessInfo::new(sysinfo_system, child, name, self.category.as_slice())
                            .await,
                    );
                }
            }
        }

        children
    }
}
