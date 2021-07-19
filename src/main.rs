#![feature(drain_filter)]

use clap::{AppSettings, Clap};
use futures::stream::StreamExt;
use heim::process::{CpuUsage, Pid, Process, Status};
use heim::units::ratio;
use plotters::prelude::*;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::process;
use std::time::Duration;
#[cfg(target_os = "windows")]
use timeout_readwrite::TimeoutReadExt;
use timeout_readwrite::TimeoutReader;

fn main() {
    let opts: Opts = Opts::parse();

    let processes = futures::executor::block_on(async {
        let mut processes = opts.find_processes().await;

        if processes.is_empty() {
            return vec![];
        }

        let mut powershell = None;

        #[cfg(target_os = "windows")]
        if opts.category.contains(&"gpu".to_owned()) {
            powershell = Some(Powershell::new());
        }

        for _ in 0..opts.times {
            futures_timer::Delay::new(Duration::from_secs(opts.interval)).await;

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
                            if let Some(gpu_percent) = process.poll_gpu_percent(powershell.as_mut())
                            {
                                process.value_percents[idx].push(gpu_percent);

                                message.push_str(&format!(" / GPU {:.2}%", gpu_percent));
                            } else {
                                continue 'p;
                            }
                        }
                        _ => unimplemented!(),
                    }
                }

                println!("{}", message);
            }
            println!("================");

            processes.drain_filter(|p| !p.valid);
        }

        processes
    });

    if processes.is_empty() {
        return;
    }

    let output = if let Some(output) = opts.output {
        output
    } else {
        return;
    };

    let root = SVGBackend::new(output.as_str(), (1280, 720 * opts.category.len() as u32))
        .into_drawing_area();
    root.fill(&WHITE).unwrap();

    let areas = root.split_evenly((opts.category.len(), 1));

    for (idx_c, area) in areas.into_iter().enumerate() {
        let mut max = 0.0f32;
        for process in processes.iter() {
            for p in &process.value_percents[idx_c] {
                max = max.max(*p);
            }
        }

        let caption;
        let unit;

        match opts.category[idx_c].as_str() {
            "cpu" => {
                caption = "Process CPU Usage";
                unit = "%";
                max = max.max(100.0);
            }
            "mem" => {
                caption = "Process MEM Usage";
                unit = "M";
                max += 100.0;
            }
            "gpu" => {
                caption = "Process GPU Usage";
                unit = "%";
                max = max.max(100.0);
            }
            _ => unimplemented!(),
        };

        let mut chart = ChartBuilder::on(&area)
            .caption(caption, ("sans-serif", 30).into_font())
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(0..(opts.times - 1), 0f32..max)
            .unwrap();

        chart
            .configure_mesh()
            .y_label_formatter(&|y| format!("{}{}", y, unit))
            .draw()
            .unwrap();

        for (idx, process) in processes.iter().enumerate() {
            let color = Palette99::pick(idx).filled();
            chart
                .draw_series(LineSeries::new(
                    process.value_percents[idx_c]
                        .clone()
                        .into_iter()
                        .enumerate(),
                    color.clone(),
                ))
                .unwrap()
                .label(format!(
                    "{}({}) / AVG({:.2}{})",
                    &process.name,
                    process.process.pid(),
                    process.avg_percent(idx_c),
                    unit
                ))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();
    }
}

struct ProcessInfo {
    process: Process,
    name: String,
    value_percents: Vec<Vec<f32>>,
    prev_cpu_usage: CpuUsage,
    valid: bool,
}

impl ProcessInfo {
    async fn new(process: Process, name: String, categories: &[String]) -> Self {
        let prev_cpu_usage = process.cpu_usage().await.unwrap();

        Self {
            process,
            name,
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
            Some((m.rss().value as f64 / 1_000_000.0) as _)
        } else {
            self.valid = false;
            None
        }
    }

    fn poll_gpu_percent(&mut self, powershell: Option<&mut Powershell>) -> Option<f32> {
        let powershell = powershell?;
        let r = powershell.poll_gpu_percent(self.process.pid());
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

struct Powershell {
    process: process::Child,
    stdout: BufReader<TimeoutReader<process::ChildStdout>>,
}

impl Powershell {
    #[cfg(target_os = "windows")]
    fn new() -> Self {
        let mut p = process::Command::new("powershell")
            .args(&["-Command", "-"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .unwrap();
        let o = BufReader::new(
            p.stdout
                .take()
                .unwrap()
                .with_timeout(Duration::from_secs(1)),
        );
        Self {
            process: p,
            stdout: o,
        }
    }

    fn poll_gpu_percent(&mut self, pid: Pid) -> Option<f32> {
        let mut gpu_percent = 0.0;
        let mut r = String::new();

        let stdin = self.process.stdin.as_mut().unwrap();
        let stdout = &mut self.stdout;

        stdin.write_all(format!(
            "(Get-Counter \"\\GPU Engine(pid_{}*engtype_3D)\\Utilization Percentage\").CounterSamples.CookedValue\r\n",
            pid
        ).as_bytes()).unwrap();
        stdout.read_line(&mut r).ok()?;

        gpu_percent += r.trim().parse::<f32>().unwrap();

        r.clear();

        stdin.write_all(format!(
            "(Get-Counter \"\\GPU Engine(pid_{}*engtype_VideoEncode)\\Utilization Percentage\").CounterSamples.CookedValue\r\n",
            pid
        ).as_bytes()).unwrap();
        stdout.read_line(&mut r).ok()?;

        gpu_percent += r.trim().parse::<f32>().unwrap();

        Some(gpu_percent)
    }
}

#[derive(Clap, Debug, Clone)]
#[clap(version = "0.1.3", author = "Xiaopeng Li <x.friday@outlook.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long)]
    process: Vec<Pid>,
    #[clap(long)]
    name: Vec<String>,
    #[clap(short, long)]
    output: Option<String>,
    #[clap(short, long, default_value = "1")]
    interval: u64,
    #[clap(short, long, default_value = "30")]
    times: usize,
    #[clap(short, long, default_value = "cpu")]
    category: Vec<String>,
}

impl Opts {
    async fn find_processes(&self) -> Vec<ProcessInfo> {
        let mut processes = vec![];
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
                processes.push(ProcessInfo::new(p, name, self.category.as_slice()).await);
            } else {
                for n in self.name.iter() {
                    if name.contains(n) {
                        processes.push(ProcessInfo::new(p, name, self.category.as_slice()).await);
                        break;
                    }
                }
            }
        }

        processes
    }
}
