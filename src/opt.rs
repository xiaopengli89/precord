use crate::types::ProcessInfo;
use crate::Pid;
use clap::{Parser, ValueEnum};
use crossterm::style::Color;
use precord_core::System;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use sysinfo::{PidExt, ProcessExt, ProcessStatus, SystemExt};

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Opts {
    #[arg(short, long)]
    process: Vec<Pid>,
    #[arg(long)]
    name: Vec<String>,
    /// Specify the output file, e.g., -o result.{svg,html,json,csv}
    #[arg(short, long, value_parser)]
    pub output: Vec<PathBuf>,
    #[arg(short, long, default_value_t = 1)]
    pub interval: u64,
    #[arg(short = 'n')]
    pub count: Option<usize>,
    /// Recording time limit, e.g., --time 1h30m59s
    #[arg(long, value_parser)]
    pub time: Option<humantime::Duration>,
    #[arg(short, long, value_enum, default_value = "cpu")]
    pub category: Vec<Category>,
    #[arg(short, long)]
    recurse_children: bool,
    #[arg(long, default_value_t = 0)]
    pub skip: usize,
    #[arg(long, value_enum, default_value = "max")]
    pub gpu_calc: GpuCalculation,
}

impl Opts {
    pub fn find_processes(&self, system: &System, proc_category_len: usize) -> Vec<ProcessInfo> {
        let mut processes: Vec<ProcessInfo> = vec![];

        if self.name.is_empty() {
            for &pid in self.process.iter() {
                if processes.iter().position(|p| p.pid == pid).is_some() {
                    continue;
                }

                if let Some(p) = ProcessInfo::new(system, proc_category_len, pid) {
                    processes.push(p);
                }
            }
        } else {
            if let Some(sysinfo_system) = system.sysinfo_system() {
                for (&pid, p) in sysinfo_system.processes() {
                    let pid = pid.as_u32();
                    match p.status() {
                        ProcessStatus::Zombie => continue,
                        _ => {}
                    }

                    if let Some(process) = ProcessInfo::new(system, proc_category_len, pid) {
                        if self.process.contains(&pid) {
                            processes.push(process);
                        } else {
                            for n in self.name.iter() {
                                if process.name.contains(n) {
                                    processes.push(process);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.recurse_children {
            processes.extend(self.recurse_children(
                system,
                processes.as_slice(),
                proc_category_len,
            ));
        }

        processes
    }

    fn recurse_children(
        &self,
        system: &System,
        processes: &[ProcessInfo],
        proc_category_len: usize,
    ) -> Vec<ProcessInfo> {
        let recurse_parent = |mut parent: Pid| {
            if processes.iter().position(|p| p.pid == parent).is_some() {
                return true;
            }

            if let Some(sysinfo_system) = system.sysinfo_system() {
                let mut visited = HashSet::new();
                visited.insert(parent);

                while let Some(parent_process) =
                    sysinfo_system.process(sysinfo::Pid::from_u32(parent))
                {
                    if let Some(parent2) = parent_process.parent() {
                        let parent2 = parent2.as_u32();
                        if visited.contains(&parent2) {
                            return false;
                        }

                        if processes.iter().position(|p| p.pid == parent2).is_some() {
                            return true;
                        }
                        visited.insert(parent2);
                        parent = parent2;
                    } else {
                        return false;
                    }
                }
            }
            false
        };

        let mut children = vec![];

        if let Some(sysinfo_system) = system.sysinfo_system() {
            for (&pid, child) in sysinfo_system.processes() {
                let pid = pid.as_u32();
                match child.status() {
                    ProcessStatus::Zombie => continue,
                    _ => {}
                }

                if processes.iter().position(|p| p.pid == pid).is_some() {
                    continue;
                }

                if let Some(parent) = child.parent() {
                    let parent = parent.as_u32();
                    if recurse_parent(parent) {
                        if let Some(p) = ProcessInfo::new(system, proc_category_len, pid) {
                            children.push(p);
                        }
                    } else if let Some(rpid) = system.process_responsible(pid) {
                        if processes.iter().position(|p| p.pid == rpid).is_some() {
                            if let Some(p) = ProcessInfo::new(system, proc_category_len, pid) {
                                children.push(p);
                            }
                        }
                    }
                }
            }
        }

        children
    }
}

#[derive(ValueEnum, Debug, Copy, Clone)]
#[clap(rename_all = "snake_case")]
pub enum Category {
    CPU,
    Mem,
    GPU,
    FPS,
    NetIn,
    NetOut,
    DiskRead,
    DiskWrite,
    Kobject,
    SysCPUFreq,
    SysCPUTemp,
    SysGPU,
}

impl Category {
    pub fn to_process(self) -> Option<ProcessCategory> {
        match self {
            Category::CPU => Some(ProcessCategory::CPU),
            Category::Mem => Some(ProcessCategory::Mem),
            Category::GPU => Some(ProcessCategory::GPU),
            Category::FPS => Some(ProcessCategory::FPS),
            Category::NetIn => Some(ProcessCategory::NetIn),
            Category::NetOut => Some(ProcessCategory::NetOut),
            Category::DiskRead => Some(ProcessCategory::DiskRead),
            Category::DiskWrite => Some(ProcessCategory::DiskWrite),
            Category::Kobject => Some(ProcessCategory::Kobject),
            _ => None,
        }
    }

    pub fn to_system(self) -> Option<SystemCategory> {
        match self {
            Category::SysCPUFreq => Some(SystemCategory::CPUFreq),
            Category::SysCPUTemp => Some(SystemCategory::CPUTemp),
            Category::SysGPU => Some(SystemCategory::GPU),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProcessCategory {
    CPU,
    Mem,
    GPU,
    FPS,
    NetIn,
    NetOut,
    DiskRead,
    DiskWrite,
    Kobject,
}

impl ProcessCategory {
    pub fn unit(&self) -> &'static str {
        match self {
            Self::CPU => "%",
            Self::Mem => "M",
            Self::GPU => "%",
            Self::FPS => "",
            Self::NetIn => "KBps",
            Self::NetOut => "KBps",
            Self::DiskRead => "KBps",
            Self::DiskWrite => "KBps",
            Self::Kobject => "",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::CPU => Color::DarkGreen,
            Self::Mem => Color::DarkCyan,
            Self::GPU => Color::AnsiValue(208),
            Self::FPS => Color::DarkYellow,
            Self::NetIn => Color::DarkBlue,
            Self::NetOut => Color::DarkMagenta,
            Self::DiskRead => Color::AnsiValue(143),
            Self::DiskWrite => Color::AnsiValue(136),
            Self::Kobject => Color::AnsiValue(215),
        }
    }

    pub fn lower_bound(&self) -> f32 {
        match self {
            Self::CPU => 100.,
            Self::Mem => 10.,
            Self::GPU => 100.,
            Self::FPS => 60.,
            Self::NetIn => (1 << 10) as _,
            Self::NetOut => (1 << 10) as _,
            Self::DiskRead => (1 << 10) as _,
            Self::DiskWrite => (1 << 10) as _,
            Self::Kobject => 100.,
        }
    }

    pub fn sample(&self, system: &mut System, gpu_calc: GpuCalculation, pid: Pid) -> Option<f32> {
        match self {
            Self::CPU => system.process_cpu_usage(pid),
            Self::Mem => system.process_mem(pid).map(|v| v / 1024.),
            Self::GPU => system.process_gpu_usage(pid, gpu_calc.into()),
            Self::FPS => Some(system.process_fps(pid)),
            Self::NetIn => system.process_net_traffic_in(pid).map(|v| (v >> 10) as f32),
            Self::NetOut => system
                .process_net_traffic_out(pid)
                .map(|v| (v >> 10) as f32),
            Self::DiskRead => system.process_disk_read(pid).map(|v| v / 1024.),
            Self::DiskWrite => system.process_disk_write(pid).map(|v| v / 1024.),
            Self::Kobject => system.process_kobject(pid).map(|v| v as _),
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum SystemCategory {
    CPUFreq,
    CPUTemp,
    GPU,
}

#[derive(ValueEnum, Debug, Copy, Clone)]
pub enum GpuCalculation {
    Max,
    Sum,
}

impl From<GpuCalculation> for precord_core::GpuCalculation {
    fn from(calc: GpuCalculation) -> Self {
        match calc {
            GpuCalculation::Max => Self::Max,
            GpuCalculation::Sum => Self::Sum,
        }
    }
}
