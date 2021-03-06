use crate::types::ProcessInfo;
use crate::Pid;
use clap::{ArgEnum, Parser};
use precord_core::System;
use std::collections::HashSet;
use std::path::PathBuf;
use sysinfo::{ProcessExt, ProcessStatus, SystemExt};

#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Opts {
    #[clap(short, long, multiple_values = true)]
    process: Vec<Pid>,
    #[clap(long, multiple_values = true)]
    name: Vec<String>,
    /// Specify the output file, e.g., -o result.{svg,html,json,csv}
    #[clap(short, long, multiple_values = true, parse(from_os_str))]
    pub output: Vec<PathBuf>,
    #[clap(short, long, default_value_t = 1)]
    pub interval: u64,
    #[clap(short = 'n')]
    pub count: Option<usize>,
    /// Recording time limit, e.g., --time 1h30m59s
    #[clap(long, parse(try_from_str))]
    pub time: Option<humantime::Duration>,
    #[clap(short, long, multiple_values = true, arg_enum, default_value = "cpu")]
    pub category: Vec<Category>,
    #[clap(short, long)]
    recurse_children: bool,
    #[clap(long, default_value_t = 0)]
    pub skip: usize,
    #[clap(long, arg_enum, default_value = "max")]
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

                processes.push(ProcessInfo::new(system, proc_category_len, pid));
            }
        } else {
            if let Some(sysinfo_system) = system.sysinfo_system() {
                for (&pid, p) in sysinfo_system.processes() {
                    match p.status() {
                        ProcessStatus::Zombie => continue,
                        _ => {}
                    }

                    let process = ProcessInfo::new(system, proc_category_len, pid);

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

                while let Some(parent_process) = sysinfo_system.process(parent) {
                    if let Some(parent2) = parent_process.parent() {
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
                match child.status() {
                    ProcessStatus::Zombie => continue,
                    _ => {}
                }

                if processes.iter().position(|p| p.pid == pid).is_some() {
                    continue;
                }

                if let Some(parent) = child.parent() {
                    if recurse_parent(parent) {
                        children.push(ProcessInfo::new(system, proc_category_len, pid));
                    } else if let Some(rpid) = system.process_responsible(pid) {
                        if processes.iter().position(|p| p.pid == rpid).is_some() {
                            children.push(ProcessInfo::new(system, proc_category_len, pid));
                        }
                    }
                }
            }
        }

        children
    }
}

#[derive(ArgEnum, Debug, Copy, Clone)]
pub enum Category {
    CPU,
    Mem,
    GPU,
    FPS,
    #[clap(name = "net_in")]
    NetIn,
    #[clap(name = "net_out")]
    NetOut,
    #[clap(name = "sys_cpu_freq")]
    SysCPUFreq,
    #[clap(name = "sys_cpu_temp")]
    SysCPUTemp,
    #[clap(name = "sys_gpu")]
    SysGPU,
}

#[derive(Copy, Clone, PartialEq)]
pub enum ProcessCategory {
    CPU,
    Mem,
    GPU,
    FPS,
    NetIn,
    NetOut,
}

#[derive(Copy, Clone, PartialEq)]
pub enum SystemCategory {
    CPUFreq,
    CPUTemp,
    GPU,
}

#[derive(ArgEnum, Debug, Copy, Clone)]
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
