use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::{ProcessInfo, SystemMetrics};
use plotters::prelude::*;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    output: P,
    proc_category: &[ProcessCategory],
    sys_category: &[SystemCategory],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    system_metrics: &[SystemMetrics],
) {
    if timestamps.is_empty() {
        return;
    }

    let timestamp_range = || timestamps[0].clone()..timestamps.last().cloned().unwrap();

    let top_height = if !processes.is_empty() {
        (processes.len() + 2) * 15
    } else {
        0
    };

    let root = SVGBackend::new(
        &output,
        (
            1280,
            (top_height + 720 * (proc_category.len() + sys_category.len())) as u32,
        ),
    )
    .into_drawing_area();
    root.fill(&WHITE).unwrap();

    let (top, bottom) = root.split_vertically(top_height as u32);
    let default_font = ("sans-serif", 12).into_font();
    let default_style: TextStyle = default_font.into();

    for (i, p) in processes.into_iter().enumerate() {
        let color = Palette99::pick(i).stroke_width(2).filled();
        let i = i as i32;
        let legend = PathElement::new(vec![(60, 23 + i * 15), (80, 23 + i * 15)], color);
        top.draw(&legend).unwrap();
        let label = format!("{}({}) - {}", p.name, p.pid, p.command);
        top.draw_text(&label, &default_style, (90, 19 + i * 15))
            .unwrap();
    }

    let areas = bottom.split_evenly((proc_category.len() + sys_category.len(), 1));

    // Draw process
    for idx_c in 0..proc_category.len() {
        let area = &areas[idx_c];
        let mut total = vec![];

        for process in processes.iter() {
            if total.len() < process.values[idx_c].len() {
                total.extend_from_slice(
                    vec![0.0; process.values[idx_c].len() - total.len()].as_slice(),
                );
            }
            for (a, b) in total.iter_mut().zip(process.values[idx_c].iter()) {
                *a += *b;
            }
        }
        let mut max = 0.0f32;
        for &t in total.iter() {
            max = max.max(t);
        }

        max = max.max(proc_category[idx_c].lower_bound());

        let mut chart = ChartBuilder::on(&area)
            .caption(
                format!("Process {:?}", proc_category[idx_c]),
                ("sans-serif", 30).into_font(),
            )
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(timestamp_range(), 0f32..max)
            .unwrap();

        chart
            .configure_mesh()
            .y_label_formatter(&|y| format!("{}{}", y, proc_category[idx_c].unit()))
            .draw()
            .unwrap();

        for (idx, process) in processes.iter().enumerate() {
            let color = Palette99::pick(idx).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    timestamps
                        .into_iter()
                        .cloned()
                        .zip(process.values[idx_c].iter().cloned()),
                    color.clone(),
                ))
                .unwrap()
                .label(format!(
                    "{}({}) / AVG({:.2}{})",
                    &process.name,
                    process.pid,
                    process.avg_value(idx_c),
                    proc_category[idx_c].unit(),
                ))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        if processes.len() > 1 {
            // Total
            let avg: f32 = total.iter().map(|a| *a).sum::<f32>() / total.len() as f32;
            let color = Palette99::pick(processes.len()).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    timestamps.into_iter().cloned().zip(total.into_iter()),
                    color.clone(),
                ))
                .unwrap()
                .label(format!(
                    "Total / AVG({:.2}{})",
                    avg,
                    proc_category[idx_c].unit()
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

    // Draw system
    for (i, &c) in sys_category.into_iter().enumerate() {
        let area = &areas[proc_category.len() + i];
        let mut chart;

        let metrics = &system_metrics[i];
        let max = metrics.max().unwrap_or(0.).max(c.lower_bound());

        chart = ChartBuilder::on(&area)
            .caption(format!("System {:?}", c), ("sans-serif", 30).into_font())
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(timestamp_range(), 0f32..max)
            .unwrap();

        chart
            .configure_mesh()
            .y_label_formatter(&|y| format!("{}{}", y, c.unit()))
            .draw()
            .unwrap();

        for (idx, row) in metrics.rows.iter().enumerate() {
            let color = Palette99::pick(idx).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    timestamps.into_iter().cloned().zip(row.iter().copied()),
                    color.clone(),
                ))
                .unwrap()
                .label(format!(
                    "{:?}{} / AVG({:.2}{})",
                    c,
                    idx,
                    metrics.row_avg(idx).unwrap_or(0.),
                    c.unit()
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
