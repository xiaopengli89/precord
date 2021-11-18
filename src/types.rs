pub struct GpuInfo {
    pub utilization: Vec<f32>,
}

impl GpuInfo {
    pub fn utilization_avg(&self) -> f32 {
        if self.utilization.is_empty() {
            0.0
        } else {
            self.utilization.iter().sum::<f32>() / (self.utilization.len() as f32)
        }
    }
}
