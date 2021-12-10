use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, Opts};
use plotters::prelude::*;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    output: &P,
    opts: &Opts,
    sys_category: &[String],
    processes: &[ProcessInfo],
    cpu_info: &[CpuInfo],
    cpu_frequency_max: f32,
    gpu_info: &[GpuInfo],
) {
    let root = SVGBackend::new(
        output,
        (
            1280,
            720 * (opts.category.len() + sys_category.len()) as u32,
        ),
    )
    .into_drawing_area();
    root.fill(&WHITE).unwrap();

    let areas = root.split_evenly((opts.category.len() + sys_category.len(), 1));

    // Draw process
    for idx_c in 0..opts.category.len() {
        let area = &areas[idx_c];
        let mut total = vec![];

        for process in processes.iter() {
            if total.len() < process.value_percents[idx_c].len() {
                total.extend_from_slice(
                    vec![0.0; process.value_percents[idx_c].len() - total.len()].as_slice(),
                );
            }
            for (a, b) in total.iter_mut().zip(process.value_percents[idx_c].iter()) {
                *a += *b;
            }
        }
        let mut max = 0.0f32;
        for &t in total.iter() {
            max = max.max(t);
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
            "fps" => {
                caption = "Process FPS";
                unit = "";
                max = max.max(60.0);
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
            let color = Palette99::pick(idx).stroke_width(2).filled();
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
                    process.pid,
                    process.avg_percent(idx_c),
                    unit
                ))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        if processes.len() > 1 {
            // Total
            let avg: f32 = total.iter().map(|a| *a).sum::<f32>() / total.len() as f32;
            let color = Palette99::pick(processes.len()).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    total.into_iter().enumerate(),
                    color.clone(),
                ))
                .unwrap()
                .label(format!("Total / AVG({:.2}{})", avg, unit))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();
    }

    // Draw system
    let mut area_i = opts.category.len();

    for c in sys_category {
        let area = &areas[area_i];
        let mut chart;

        match c.as_str() {
            "sys_cpu_freq" => {
                chart = ChartBuilder::on(&area)
                    .caption("CPU Frequency", ("sans-serif", 30).into_font())
                    .margin(10)
                    .x_label_area_size(40)
                    .y_label_area_size(50)
                    .build_cartesian_2d(0..(opts.times - 1), 0f32..cpu_frequency_max)
                    .unwrap();

                chart
                    .configure_mesh()
                    .y_label_formatter(&|y| format!("{}MHz", y))
                    .draw()
                    .unwrap();

                for (idx, info) in cpu_info.iter().enumerate() {
                    let color = Palette99::pick(idx).stroke_width(2).filled();
                    chart
                        .draw_series(LineSeries::new(
                            info.freq.clone().into_iter().enumerate(),
                            color.clone(),
                        ))
                        .unwrap()
                        .label(format!("CPU{} / AVG({:.2}MHz)", idx, info.avg(),))
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], color.clone())
                        });
                }
            }
            "sys_gpu" => {
                chart = ChartBuilder::on(&area)
                    .caption("System GPU Utilization", ("sans-serif", 30).into_font())
                    .margin(10)
                    .x_label_area_size(40)
                    .y_label_area_size(50)
                    .build_cartesian_2d(0..(opts.times - 1), 0.0f32..100.0f32)
                    .unwrap();

                chart
                    .configure_mesh()
                    .y_label_formatter(&|y| format!("{}%", y))
                    .draw()
                    .unwrap();

                for (idx, info) in gpu_info.iter().enumerate() {
                    let color = Palette99::pick(idx).stroke_width(2).filled();
                    chart
                        .draw_series(LineSeries::new(
                            info.utilization.clone().into_iter().enumerate(),
                            color.clone(),
                        ))
                        .unwrap()
                        .label(format!("GPU / AVG({:.2}%)", info.utilization_avg()))
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], color.clone())
                        });
                }
            }
            _ => unreachable!(),
        }

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();

        area_i += 1;
    }
}
