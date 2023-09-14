use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::{ProcessInfo, SystemMetrics};
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const CHART_HEIGHT: usize = 800;
const CHART_PADDING_LEFT: usize = 50;
const CHART_PADDING_RIGHT: usize = 300;
const CHART_PADDING_TOP_BOTTOM: usize = 100;

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

    let mut titles = vec![];
    let mut grids = vec![];
    let mut x_axis = vec![];
    let mut y_axis = vec![];
    let mut series = vec![];
    let mut legends = vec![];
    let mut data_zooms = vec![];
    let mut tooltips = vec![];

    for ci in 0..proc_category.len() {
        let mut max_value: f32 = proc_category[ci].lower_bound();
        let category_title = format!("Process {:?}", proc_category[ci]);
        let unit = proc_category[ci].unit();
        let mut total = vec![];
        let mut legend_c = vec![];
        let mut tooltip = HashMap::new();

        for p in processes {
            let avg = p.avg_value(ci);

            total.extend_from_slice(
                vec![0.0; p.values[ci].len().saturating_sub(total.len())].as_slice(),
            );

            let data: Vec<_> = p.values[ci]
                .iter()
                .copied()
                .enumerate()
                .zip(timestamps.into_iter())
                .map(|((i, v), t)| {
                    max_value = max_value.max(v);
                    total[i] += v;

                    json!([t, v])
                })
                .collect();
            let name = format!("{} / AVG({:.2}{}) / {}", p.pid, avg, unit, &p.name);
            series.push(json!({
                "name": &name,
                "type": "line",
                "showSymbol": false,
                "xAxisIndex": grids.len(),
                "yAxisIndex": grids.len(),
                "data": data,
                "markLine": {
                    "data": [{"type": "average"}],
                },
                "emphasis": {
                    "focus": "series",
                },
            }));
            legend_c.push(json!({
                "name": &name,
            }));
            tooltip.insert(name, p.command.clone());
        }

        if processes.len() > 1 {
            let avg: f32 = total.iter().copied().sum::<f32>() / total.len() as f32;

            let data: Vec<_> = total
                .into_iter()
                .zip(timestamps.into_iter())
                .map(|(v, t)| {
                    max_value = max_value.max(v);

                    json!([t, v])
                })
                .collect();
            series.push(json!({
                "name": format!("Total / AVG({:.2}{})", avg, unit),
                "type": "line",
                "showSymbol": false,
                "xAxisIndex": grids.len(),
                "yAxisIndex": grids.len(),
                "data": data,
                "markLine": {
                    "data": [{"type": "average"}],
                },
                "emphasis": {
                    "focus": "series",
                },
            }));
            legend_c.push(json!({
                "name": format!("Total / AVG({:.2}{})", avg, unit),
            }));
        }

        x_axis.push(json!({
            "gridIndex": grids.len(),
            "type": "time",
        }));
        y_axis.push(json!({
            "gridIndex": grids.len(),
            "min": 0.0,
            "max": max_value.ceil(),
            "axisLabel": {
                "formatter": format!("{{value}}{}", unit),
            },
        }));
        titles.push(json!({
            "text": category_title,
            "textAlign": "left",
            "top": format!("{}px", 800 * grids.len() + 20),
            "left": CHART_PADDING_LEFT - 10,
        }));
        legends.push(json!({
            "type": "scroll",
            "orient": "vertical",
            "right": 0,
            "top": format!("{}px", 800 * grids.len() + 100),
            "data": legend_c,
            "tooltip": {
                "show": true,
                "extraCssText": "max-width:500px; white-space:normal;",
            },
        }));
        data_zooms.push(json!({
            "xAxisIndex": [grids.len()],
            "top": format!("{}px", 800 * grids.len() + 730),
        }));
        grids.push(json!({
            "height": format!("{}px", CHART_HEIGHT - CHART_PADDING_TOP_BOTTOM * 2),
            "left": CHART_PADDING_LEFT,
            "top": format!("{}px", CHART_HEIGHT * grids.len() + CHART_PADDING_TOP_BOTTOM),
            "right": CHART_PADDING_RIGHT,
        }));
        tooltips.push(tooltip);
    }

    for (i, &sys_c) in sys_category.into_iter().enumerate() {
        let metrics = &system_metrics[i];
        let max_value = metrics.max().unwrap_or(0.).max(sys_c.lower_bound());
        let category_title = format!("System {:?}", sys_c);
        let unit = sys_c.unit();
        let mut legend_c = vec![];
        let tooltip = HashMap::new();

        for (si, row) in metrics.rows.iter().enumerate() {
            let avg = metrics.row_avg(si).unwrap_or(0.0);
            let data: Vec<_> = row
                .iter()
                .copied()
                .zip(timestamps.into_iter())
                .map(|(v, t)| json!([t, v]))
                .collect();
            let name = format!("{:?}{} / AVG({:.2}{})", sys_c, si, avg, unit);
            series.push(json!({
                "name": &name,
                "type": "line",
                "showSymbol": false,
                "xAxisIndex": grids.len(),
                "yAxisIndex": grids.len(),
                "data": data,
                "markLine": {
                    "data": [{"type": "average"}],
                },
                "emphasis": {
                    "focus": "series",
                },
            }));
            legend_c.push(json!({
                "name": &name,
            }));
        }

        x_axis.push(json!({
            "gridIndex": grids.len(),
            "type": "time",
        }));
        y_axis.push(json!({
            "gridIndex": grids.len(),
            "min": 0.0,
            "max": max_value.ceil(),
            "axisLabel": {
                "formatter": format!("{{value}}{}", unit),
            },
        }));
        titles.push(json!({
            "text": category_title,
            "textAlign": "left",
            "top": format!("{}px", 800 * grids.len()),
            "left": CHART_PADDING_LEFT - 10,
        }));
        legends.push(json!({
            "type": "scroll",
            "orient": "vertical",
            "right": 0,
            "top": format!("{}px", 800 * grids.len() + 100),
            "data": legend_c,
            "tooltip": {
                "show": true,
                "extraCssText": "max-width:500px; white-space:normal;",
            },
        }));
        data_zooms.push(json!({
            "xAxisIndex": [grids.len()],
            "top": format!("{}px", 800 * grids.len() + 730),
        }));
        grids.push(json!({
            "height": format!("{}px", CHART_HEIGHT - CHART_PADDING_TOP_BOTTOM * 2),
            "left": CHART_PADDING_LEFT,
            "top": format!("{}px", CHART_HEIGHT * grids.len() + CHART_PADDING_TOP_BOTTOM),
            "right": CHART_PADDING_RIGHT,
        }));
        tooltips.push(tooltip);
    }

    let grid_len = grids.len();
    let option = json!({
        "tooltip": {
            "show": true,
            "trigger": "axis",
        },
        "grid": grids,
        "title": titles,
        "xAxis": x_axis,
        "yAxis": y_axis,
        "legend": legends,
        "series": series,
        "dataZoom": data_zooms,
    });

    let html_content = r#"
   <!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <script src="https://cdn.jsdelivr.net/npm/echarts@5.2.2/dist/echarts.min.js"></script>
    <style>
        #main {
            margin: 20px auto;
        }
    </style>
  </head>
  <body>
    <div id="main" style="height: "#
        .to_string()
        + &(800 * grid_len).to_string()
        + &r#"px;"></div>
    <script>
      var myChart = echarts.init(document.getElementById('main'), null, { renderer: 'svg' });
      var option = "#
            .to_string()
        + &option.to_string()
        + r#";

      var tooltips = "#
        + &serde_json::to_string(&tooltips).unwrap()
        + r#";
      option.legend.forEach(function(l, i) {
        l.tooltip.formatter = function(name) {
            console.log(name);
            var t = tooltips[i][name.name];
            t = t ? "<br/>" + t : "";
            return "<b>" + name.name + "</b>" + t;
        };
      });

      myChart.setOption(option);
      window.addEventListener('resize', function() {
        myChart.resize();
      });
    </script>
  </body>
</html>
    "#;

    let mut file = File::create(output).unwrap();
    file.write_all(html_content.as_bytes()).unwrap();
    file.sync_all().unwrap();
}
