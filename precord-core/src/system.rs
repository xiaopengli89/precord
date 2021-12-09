#[cfg(target_os = "macos")]
use crate::platform::macos::IOKitRegistry;
#[cfg(target_os = "macos")]
use crate::platform::macos::PowerMetrics;
#[cfg(target_os = "windows")]
use crate::platform::windows::EtwTrace;
#[cfg(target_os = "windows")]
use crate::platform::windows::{Pdh, ProcessorInfo};
use crate::Pid;
use bitflags::bitflags;
use sysinfo::{ProcessExt, RefreshKind, SystemExt};

#[derive(Default)]
pub struct System {
    sysinfo_system: Option<sysinfo::System>,
    #[cfg(target_os = "macos")]
    power_metrics: Option<PowerMetrics>,
    #[cfg(target_os = "macos")]
    ioreg: Option<IOKitRegistry>,
    #[cfg(target_os = "windows")]
    pdh: Option<Pdh>,
    #[cfg(target_os = "windows")]
    wmi_con: Option<wmi::WMIConnection>,
    #[cfg(target_os = "windows")]
    etw_trace: Option<EtwTrace>,
}

impl System {
    #[allow(unused_variables)]
    pub fn new<T: IntoIterator<Item = Pid>>(features: Features, pids: T) -> Self {
        let mut system = System::default();

        if features.contains(Features::PROCESS) {
            system.sysinfo_system = Some(sysinfo::System::new_with_specifics(
                RefreshKind::new().with_processes(),
            ));
        }

        if features.contains(Features::GPU) {
            #[cfg(target_os = "macos")]
            {
                system.ioreg = Some(IOKitRegistry::new());
            }
            #[cfg(target_os = "windows")]
            {
                system.pdh = Some(Pdh::new(pids));
            }
        }

        if features.contains(Features::CPU_FREQUENCY) {
            #[cfg(target_os = "macos")]
            {
                system.power_metrics = Some(PowerMetrics::new());
            }
            #[cfg(target_os = "windows")]
            {
                system.wmi_con =
                    Some(wmi::WMIConnection::new(wmi::COMLibrary::new().unwrap().into()).unwrap());
            }
        }

        if features.contains(Features::FPS) {
            #[cfg(target_os = "windows")]
            {
                system.etw_trace = Some(EtwTrace::new());
            }
        }

        system
    }

    pub fn update(&mut self) {
        if let Some(sysinfo_system) = &mut self.sysinfo_system {
            sysinfo_system.refresh_processes();
        }

        #[cfg(target_os = "macos")]
        if let Some(power_metrics) = &mut self.power_metrics {
            power_metrics.poll();
        }

        #[cfg(target_os = "macos")]
        if let Some(ioreg) = &mut self.ioreg {
            ioreg.poll();
        }

        #[cfg(target_os = "windows")]
        if let Some(pdh) = &mut self.pdh {
            pdh.update();
        }
    }

    pub fn sysinfo_system(&self) -> Option<&sysinfo::System> {
        self.sysinfo_system.as_ref()
    }

    pub fn process_cpu_utilization(&self, pid: Pid) -> Option<f32> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.cpu_usage())
    }

    pub fn process_mem(&self, pid: Pid) -> Option<f32> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.memory() as f32)
    }

    pub fn process_name(&self, pid: Pid) -> Option<&str> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.name())
    }

    pub fn process_command(&self, pid: Pid) -> Option<&[String]> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.cmd())
    }

    pub fn cpu_frequency(&self) -> Vec<f32> {
        #[cfg(target_os = "macos")]
        {
            self.power_metrics.as_ref().unwrap().cpu_frequency()
        }
        #[cfg(target_os = "windows")]
        {
            let processor_info: Vec<ProcessorInfo> = self.wmi_con.as_ref().unwrap().raw_query("SELECT Name, PercentProcessorPerformance, ProcessorFrequency FROM Win32_PerfFormattedData_Counters_ProcessorInformation WHERE NOT Name LIKE '%_Total\'
").unwrap();
            processor_info
                .into_iter()
                .map(|p| p.processor_frequency * p.percent_processor_performance / 100.0)
                .collect()
        }
    }

    #[allow(unused_variables)]
    pub fn process_gpu_percent(&mut self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            Some(0.0)
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_percent(Some(pid))
        }
    }

    #[allow(unused_variables)]
    pub fn process_fps(&mut self, pid: Pid) -> f32 {
        #[cfg(target_os = "macos")]
        {
            0.0
        }

        #[cfg(target_os = "windows")]
        {
            self.etw_trace.as_mut().unwrap().fps(pid)
        }
    }

    pub fn system_gpu_percent(&mut self) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            Some(self.ioreg.as_ref().unwrap().sys_gpu_utilization())
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_percent(None)
        }
    }
}

bitflags! {
    pub struct Features: u32 {
        const PROCESS =         1 << 0;
        const GPU =             1 << 1;
        const CPU_FREQUENCY =   1 << 2;
        const FPS =             1 << 3;
    }
}
