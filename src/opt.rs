use crate::types::ProcessInfo;
use crate::Pid;
use clap::{ArgEnum, Parser};
use precord_core::System;
use std::path::PathBuf;
use sysinfo::{ProcessExt, ProcessStatus, SystemExt};

#[derive(Parser, Debug, Clone)]
#[clap(version = "0.3.6")]
#[clap(about = "A command line tool to record process and system performance data.")]
pub struct Opts {
    #[clap(short, long, multiple_values = true)]
    process: Vec<Pid>,
    #[clap(long, multiple_values = true)]
    name: Vec<String>,
    #[clap(short, long, multiple_values = true, parse(from_os_str))]
    pub output: Vec<PathBuf>,
    #[clap(short, long, default_value_t = 1)]
    pub interval: u64,
    #[clap(short = 'n', default_value_t = 30)]
    pub count: usize,
    #[clap(short, long, multiple_values = true, arg_enum, default_value = "cpu")]
    pub category: Vec<Category>,
    #[clap(short, long)]
    recurse_children: bool,
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
                while let Some(parent_process) = sysinfo_system.process(parent) {
                    if let Some(parent2) = parent_process.parent() {
                        if processes.iter().position(|p| p.pid == parent2).is_some() {
                            return true;
                        }
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
    #[clap(name = "sys_cpu_freq")]
    SysCPUFreq,
    #[clap(name = "sys_gpu")]
    SysGPU,
}

#[derive(Copy, Clone, PartialEq)]
pub enum ProcessCategory {
    CPU,
    Mem,
    GPU,
    FPS,
}

#[derive(Copy, Clone, PartialEq)]
pub enum SystemCategory {
    CPUFreq,
    GPU,
}
