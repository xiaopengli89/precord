use crate::types::ProcessInfo;
use crate::Pid;
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::style::Color;
use precord_core::{platform, Features, System};
use serde::Serialize;
use std::collections::HashSet;
use std::iter;
use std::path::PathBuf;
use sysinfo::{PidExt, ProcessExt, ProcessStatus, SystemExt};

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Opts {
    #[arg(short, long, num_args(..))]
    process: Vec<Pid>,
    #[arg(long, num_args(..))]
    name: Vec<String>,
    /// Specify the output file, e.g., -o result.{svg,html,json,csv}
    #[arg(short, long, value_parser, num_args(..))]
    pub output: Vec<PathBuf>,
    #[arg(short, long, default_value_t = 1)]
    pub interval: u64,
    #[arg(short = 'n')]
    pub count: Option<usize>,
    /// Recording time limit, e.g., --time 1h30m59s
    #[arg(long, value_parser)]
    pub time: Option<humantime::Duration>,
    #[arg(short, long, value_enum, default_value = "cpu", num_args(..))]
    pub category: Vec<Category>,
    #[arg(short, long)]
    recurse_children: bool,
    #[arg(long, default_value_t = 0)]
    pub skip: usize,
    #[arg(long, value_enum, default_value = "max")]
    pub gpu_calc: GpuCalculation,
    /// Interactive mode
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub interactive: bool,
    /// Interval of auto saving
    #[arg(long)]
    pub auto_save: Option<u64>,
    #[command(subcommand)]
    pub action: Option<Action>,
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

#[derive(Debug, Subcommand)]
pub enum Action {
    ThreadList { pid: Pid },
}

impl Action {
    pub fn exec(&self) {
        match self {
            Self::ThreadList { pid } => {
                #[cfg(target_os = "macos")]
                {
                    let system = System::new(Features::PROCESS, iter::once(*pid)).unwrap();
                    let name = system.process_name(*pid).unwrap();

                    let mut threads = platform::macos::threads_info(*pid);
                    threads.sort_by(|a, b| a.cpu_usage().total_cmp(&b.cpu_usage()));

                    println!("{}({})", name, pid);

                    let mut it = threads.into_iter().peekable();
                    while let Some(thread) = it.next() {
                        if it.peek().is_some() {
                            println!("  ├──Thread-{}: {:.2}%", thread.id(), thread.cpu_usage());
                        } else {
                            println!("  └──Thread-{}: {:.2}%", thread.id(), thread.cpu_usage());
                        }
                    }
                }
            }
        }
    }
}

#[derive(ValueEnum, Debug, Copy, Clone)]
#[clap(rename_all = "snake_case")]
pub enum Category {
    CPU,
    Mem,
    Alloc,
    GPU,
    Vram,
    FPS,
    NetIn,
    NetOut,
    DiskRead,
    DiskWrite,
    Kobject,
    SysCpu,
    SysCPUFreq,
    SysCPUTemp,
    SysGPU,
}

impl Category {
    pub fn to_process(self) -> Option<ProcessCategory> {
        match self {
            Category::CPU => Some(ProcessCategory::Cpu),
            Category::Mem => Some(ProcessCategory::Mem),
            Category::Alloc => Some(ProcessCategory::Alloc),
            Category::GPU => Some(ProcessCategory::Gpu),
            Category::Vram => Some(ProcessCategory::Vram),
            Category::FPS => Some(ProcessCategory::Fps),
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
            Category::SysCpu => Some(SystemCategory::Cpu),
            Category::SysCPUFreq => Some(SystemCategory::CpuFreq),
            Category::SysCPUTemp => Some(SystemCategory::CpuTemp),
            Category::SysGPU => Some(SystemCategory::Gpu),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProcessCategory {
    Cpu,
    Mem,
    Alloc,
    Gpu,
    Vram,
    Fps,
    NetIn,
    NetOut,
    DiskRead,
    DiskWrite,
    Kobject,
}

impl ProcessCategory {
    pub fn unit(&self) -> &'static str {
        match self {
            Self::Cpu => "%",
            Self::Mem => "M",
            Self::Alloc => "M",
            Self::Gpu => "%",
            Self::Vram => "M",
            Self::Fps => "",
            Self::NetIn => "KBps",
            Self::NetOut => "KBps",
            Self::DiskRead => "KBps",
            Self::DiskWrite => "KBps",
            Self::Kobject => "",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Cpu => Color::DarkGreen,
            Self::Mem => Color::DarkCyan,
            Self::Alloc => Color::AnsiValue(125),
            Self::Gpu => Color::AnsiValue(208),
            Self::Vram => Color::AnsiValue(64),
            Self::Fps => Color::DarkYellow,
            Self::NetIn => Color::DarkBlue,
            Self::NetOut => Color::DarkMagenta,
            Self::DiskRead => Color::AnsiValue(143),
            Self::DiskWrite => Color::AnsiValue(136),
            Self::Kobject => Color::AnsiValue(215),
        }
    }

    pub fn lower_bound(&self) -> f32 {
        match self {
            Self::Cpu => 100.,
            Self::Mem => 10.,
            Self::Alloc => 10.,
            Self::Gpu => 100.,
            Self::Vram => 10.,
            Self::Fps => 60.,
            Self::NetIn => (1 << 10) as _,
            Self::NetOut => (1 << 10) as _,
            Self::DiskRead => (1 << 10) as _,
            Self::DiskWrite => (1 << 10) as _,
            Self::Kobject => 100.,
        }
    }

    pub fn sample(&self, system: &mut System, gpu_calc: GpuCalculation, pid: Pid) -> Option<f32> {
        match self {
            Self::Cpu => system.process_cpu_usage(pid),
            Self::Mem => system.process_mem(pid).map(|v| (v >> 10) as f32 / 1024.),
            Self::Alloc => system.process_alloc(pid).map(|v| (v >> 10) as f32 / 1024.),
            Self::Gpu => system.process_gpu_usage(pid, gpu_calc.into()),
            Self::Vram => system
                .process_vram(pid, gpu_calc.into())
                .map(|v| v / (1 << 20) as f32),
            Self::Fps => Some(system.process_fps(pid)),
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SystemCategory {
    Cpu,
    CpuFreq,
    CpuTemp,
    Gpu,
}

impl SystemCategory {
    pub fn unit(&self) -> &'static str {
        match self {
            Self::Cpu => "%",
            Self::CpuFreq => "MHz",
            Self::CpuTemp => "°C",
            Self::Gpu => "%",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Cpu => Color::DarkGreen,
            Self::CpuFreq => Color::DarkCyan,
            Self::CpuTemp => Color::AnsiValue(208),
            Self::Gpu => Color::AnsiValue(64),
        }
    }

    pub fn lower_bound(&self) -> f32 {
        match self {
            Self::Cpu => 100.,
            Self::CpuFreq => 1000.,
            Self::CpuTemp => 100.,
            Self::Gpu => 100.,
        }
    }

    pub fn sample(&self, system: &mut System, gpu_calc: GpuCalculation) -> Vec<f32> {
        match self {
            Self::Cpu => system.system_cpu_usage().unwrap(),
            Self::CpuFreq => system.system_cpu_frequency().unwrap(),
            Self::CpuTemp => system.system_cpu_temperature().unwrap(),
            Self::Gpu => {
                vec![system.system_gpu_usage(gpu_calc.into()).unwrap_or(0.0)]
            }
        }
    }
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
