#[cfg(target_os = "macos")]
use crate::platform::macos::PowerMetrics;
#[cfg(target_os = "windows")]
use crate::platform::windows::{Powershell, ProcessorInfo};
use bitflags::bitflags;
use heim::process::Pid;

pub struct System {
    #[cfg(target_os = "macos")]
    power_metrics: Option<PowerMetrics>,
    #[cfg(target_os = "windows")]
    power_shell: Option<Powershell>,
    #[cfg(target_os = "windows")]
    wmi_con: Option<wmi::WMIConnection>,
}

impl System {
    pub fn new(features: Features) -> Self {
        let mut system = System {
            #[cfg(target_os = "macos")]
            power_metrics: None,
            #[cfg(target_os = "windows")]
            power_shell: None,
            #[cfg(target_os = "windows")]
            wmi_con: None,
        };

        if features.contains(Features::GPU) {
            #[cfg(target_os = "macos")]
            {
                system.power_metrics = Some(PowerMetrics::new());
            }
            #[cfg(target_os = "windows")]
            {
                system.power_shell = Some(Powershell::new());
            }
        }

        if features.contains(Features::CPU_FREQUENCY) {
            #[cfg(target_os = "macos")]
            if system.power_metrics.is_none() {
                system.power_metrics = Some(PowerMetrics::new());
            }
            #[cfg(target_os = "windows")]
            {
                system.wmi_con =
                    Some(wmi::WMIConnection::new(wmi::COMLibrary::new().unwrap().into()).unwrap());
            }
        }

        system
    }

    pub fn update(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(power_metrics) = &mut self.power_metrics {
            power_metrics.poll();
        }
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

    pub fn process_gpu_percent(&mut self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            self.power_metrics.as_ref().unwrap().gpu_percent(pid)
        }

        #[cfg(target_os = "windows")]
        {
            self.power_shell.as_mut().unwrap().poll_gpu_percent(pid)
        }
    }
}

bitflags! {
    pub struct Features: u32 {
        const GPU = 1 << 0;
        const CPU_FREQUENCY = 1 << 1;
    }
}
