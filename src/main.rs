use clap::{AppSettings, Clap};
use plotters::prelude::*;
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessExt, SystemExt};

fn main() {
    let opts: Opts = Opts::parse();

    if opts.process.is_empty() {
        return;
    }

    let pids = opts.process;
    let output = opts.output;
    let interval = opts.interval;
    let times = opts.times;

    let mut system =
        sysinfo::System::new_with_specifics(sysinfo::RefreshKind::new().with_processes());

    let mut processes: Vec<ProcessInfo> = pids
        .into_iter()
        .map(|pid| {
            system.refresh_process(pid);
            ProcessInfo::new(pid, system.process(pid).unwrap().name().to_owned())
        })
        .collect();

    for _ in 0..times {
        thread::sleep(Duration::from_secs(interval));

        for process in processes.iter_mut() {
            let cpu_percent = process.poll_cpu_percent(&mut system);
            println!("{}({}): {:.2}%", &process.name, process.pid, cpu_percent);
        }
        println!("================");
    }

    let output = if let Some(output) = output {
        output
    } else {
        return;
    };

    let mut max = 100.0f32;
    for process in processes.iter() {
        for p in &process.cpu_percents {
            max = max.max(*p);
        }
    }

    let root = SVGBackend::new(output.as_str(), (1280, 720)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let mut chart = ChartBuilder::on(&root)
        .caption("Process CPU Usage", ("sans-serif", 30).into_font())
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0..(times - 1), 0f32..max)
        .unwrap();

    chart
        .configure_mesh()
        .y_label_formatter(&|y| format!("{}%", y))
        .draw()
        .unwrap();

    for (idx, process) in processes.into_iter().enumerate() {
        let color = Palette99::pick(idx).filled();

        chart
            .draw_series(LineSeries::new(
                process.cpu_percents.clone().into_iter().enumerate(),
                color.clone(),
            ))
            .unwrap()
            .label(format!(
                "{}({}) / AVG({:.2}%)",
                &process.name,
                process.pid,
                process.avg_cpu_percent()
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

struct ProcessInfo {
    pid: sysinfo::Pid,
    name: String,
    cpu_percents: Vec<f32>,
}

impl ProcessInfo {
    fn new(pid: sysinfo::Pid, name: String) -> Self {
        Self {
            pid,
            name,
            cpu_percents: vec![],
        }
    }

    fn poll_cpu_percent(&mut self, system: &mut sysinfo::System) -> f32 {
        system.refresh_process(self.pid);
        let process = system.process(self.pid).unwrap();

        let cpu_percent = process.cpu_usage();
        self.cpu_percents.push(cpu_percent);
        cpu_percent
    }

    fn avg_cpu_percent(&self) -> f32 {
        if self.cpu_percents.is_empty() {
            0.0
        } else {
            self.cpu_percents.iter().sum::<f32>() / (self.cpu_percents.len() as f32)
        }
    }
}

#[derive(Clap, Debug)]
#[clap(version = "1.0", author = "Xiaopeng Li <x.friday@outlook.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long)]
    process: Vec<sysinfo::Pid>,
    #[clap(short, long)]
    output: Option<String>,
    #[clap(short, long, default_value = "1")]
    interval: u64,
    #[clap(short, long, default_value = "30")]
    times: usize,
}
