#[cfg(target_os = "macos")]
use crate::platform::macos::PowerMetrics;
#[cfg(target_os = "windows")]
use crate::platform::windows::Powershell;
use bitflags::bitflags;
use heim::process::Pid;

pub struct System {
    #[cfg(target_os = "macos")]
    power_metrics: Option<PowerMetrics>,
    #[cfg(target_os = "windows")]
    power_shell: Option<Powershell>,
}

impl System {
    pub fn new(features: Features) -> Self {
        let mut system = System {
            #[cfg(target_os = "macos")]
            power_metrics: None,
            #[cfg(target_os = "windows")]
            power_shell: None,
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

        system
    }

    pub fn update(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(power_metrics) = &mut self.power_metrics {
            power_metrics.poll();
        }
    }

    pub fn cpu_frequency(&self) -> Vec<f32> {
        todo!()
    }

    pub fn process_gpu_percent(&self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            self.power_metrics.as_ref().unwrap().gpu_percent(pid)
        }

        #[cfg(target_os = "windows")]
        {
            self.power_shell.as_ref().unwrap().poll_gpu_percent(pid)
        }
    }
}

bitflags! {
    pub struct Features: u32 {
        const GPU = 1 << 0;
        const CPU_FREQUENCY = 1 << 1;
    }
}
