use clap::{AppSettings, Clap};
use heim::process::{CpuUsage, Pid, Process};
use heim::units::ratio;
use plotters::prelude::*;
use std::io::BufReader;
#[cfg(target_os = "windows")]
use std::io::{BufRead, Write};
use std::process;
use std::time::Duration;

fn main() {
    let opts: Opts = Opts::parse();

    if opts.process.is_empty() {
        return;
    }

    let processes = futures::executor::block_on(async {
        let mut processes: Vec<ProcessInfo> = vec![];
        for &pid in opts.process.iter() {
            processes.push(ProcessInfo::new(pid, &opts.category).await);
        }

        for _ in 0..opts.times {
            futures_timer::Delay::new(Duration::from_secs(opts.interval)).await;

            for process in processes.iter_mut() {
                let mut message = format!("{}({})", &process.name, process.process.pid(),);

                for (idx, c) in opts.category.iter().enumerate() {
                    match c.as_str() {
                        "cpu" => {
                            let cpu_percent = process.poll_cpu_percent().await;

                            process.value_percents[idx].push(cpu_percent);

                            message.push_str(&format!(" / CPU {:.2}%", cpu_percent));
                        }
                        "gpu" => {
                            let gpu_percent = process.poll_gpu_percent();

                            process.value_percents[idx].push(gpu_percent);

                            message.push_str(&format!(" / GPU {:.2}%", gpu_percent));
                        }
                        _ => unimplemented!(),
                    }
                }

                println!("{}", message);
            }
            println!("================");
        }

        processes
    });

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
        let mut max = 100.0f32;
        for process in processes.iter() {
            for p in &process.value_percents[idx_c] {
                max = max.max(*p);
            }
        }

        let caption = match opts.category[idx_c].as_str() {
            "cpu" => "Process CPU Usage",
            "gpu" => "Process GPU Usage",
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
            .y_label_formatter(&|y| format!("{}%", y))
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
                    "{}({}) / AVG({:.2}%)",
                    &process.name,
                    process.process.pid(),
                    process.avg_percent(idx_c)
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
    #[allow(dead_code)]
    helper_process: Option<HelperProcess>,
}

impl ProcessInfo {
    async fn new(pid: Pid, categories: &[String]) -> Self {
        let process = heim::process::get(pid).await.unwrap();
        let name = process.name().await.unwrap();
        let prev_cpu_usage = process.cpu_usage().await.unwrap();

        #[allow(unused_mut)]
        let mut helper_process = None;

        #[cfg(target_os = "windows")]
        if categories.contains(&"gpu".to_owned()) {
            let mut h = process::Command::new("powershell")
                .args(&["-Command", "-"])
                .stdin(process::Stdio::piped())
                .stdout(process::Stdio::piped())
                .spawn()
                .unwrap();
            let o = BufReader::new(h.stdout.take().unwrap());
            helper_process = Some(HelperProcess {
                process: h,
                stdout: o,
            });
        }

        Self {
            process,
            name,
            value_percents: vec![vec![]; categories.len()],
            prev_cpu_usage,
            helper_process,
        }
    }

    async fn poll_cpu_percent(&mut self) -> f32 {
        let cpu_usage = self.process.cpu_usage().await.unwrap();
        let cpu_percent = (cpu_usage.clone()
            - std::mem::replace(&mut self.prev_cpu_usage, cpu_usage))
        .get::<ratio::percent>();

        cpu_percent
    }

    fn poll_gpu_percent(&mut self) -> f32 {
        #[allow(unused_mut)]
        let mut gpu_percent= 0.0;

        #[cfg(target_os = "windows")]
        {
            let helper_process = self.helper_process.as_mut().unwrap();
            let stdin = helper_process.process.stdin.as_mut().unwrap();
            let stdout = &mut helper_process.stdout;

            stdin.write_all(format!(r#"(Get-Counter "\GPU Engine(pid_{}*engtype_3D)\Utilization Percentage").CounterSamples.CookedValue
            "#, self.process.pid()).as_bytes()).unwrap();
            let mut r = String::new();
            stdout.read_line(&mut r).unwrap();

            gpu_percent += r.trim().parse::<f32>().unwrap();

            stdin.write_all(format!(r#"(Get-Counter "\GPU Engine(pid_{}*engtype_VideEncode)\Utilization Percentage").CounterSamples.CookedValue
            "#, self.process.pid()).as_bytes()).unwrap();
            let mut r = String::new();
            stdout.read_line(&mut r).unwrap();

            gpu_percent += r.trim().parse::<f32>().unwrap();
        }

        gpu_percent
    }

    fn avg_percent(&self, idx: usize) -> f32 {
        if self.value_percents[idx].is_empty() {
            0.0
        } else {
            self.value_percents[idx].iter().sum::<f32>() / (self.value_percents[idx].len() as f32)
        }
    }
}

#[allow(dead_code)]
struct HelperProcess {
    process: process::Child,
    stdout: BufReader<process::ChildStdout>,
}

#[derive(Clap, Debug, Clone)]
#[clap(version = "0.1.0", author = "Xiaopeng Li <x.friday@outlook.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long)]
    process: Vec<Pid>,
    #[clap(short, long)]
    output: Option<String>,
    #[clap(short, long, default_value = "1")]
    interval: u64,
    #[clap(short, long, default_value = "30")]
    times: usize,
    #[clap(short, long, default_value = "cpu")]
    category: Vec<String>,
}
