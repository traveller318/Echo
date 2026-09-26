/*!
 * SOURCE OF TRUTH KEYWORDS: FakeGraphicsAdapters, fake GPU list, no GPU, scripted GPU, GPU enumeration failure
 * WHAT:  FakeGraphicsAdapters: a GraphicsAdapters that returns a scripted list (none by default) or fails, and counts
 *        how often it was asked.
 * WHY:   The accelerator picker's rules (no GPU → CPU, remember per GPU and driver, a driver update measures again,
 *        an enumeration failure is not fatal) must be proven without a graphics card.
 * WHERE: pipeline/asr accelerator tests and every rig that builds an ASR worker.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::GraphicsAdapters,
    types::{ByteCount, GpuAdapter, PortError, PortResult},
};

#[derive(Default)]
struct GpuState {
    adapters: Vec<GpuAdapter>,
    next_error: Option<PortError>,
    lists: usize,
}

/// A scripted GPU list.
#[derive(Default)]
pub struct FakeGraphicsAdapters {
    state: Mutex<GpuState>,
}

impl FakeGraphicsAdapters {
    /// A machine without a DirectX 12 GPU.
    pub fn none() -> Self {
        Self::default()
    }

    /// A machine with these GPUs, in preference order.
    pub fn with(adapters: Vec<GpuAdapter>) -> Self {
        let fake = Self::default();
        fake.replace(adapters);
        fake
    }

    /// An integrated GPU with the given driver version.
    pub fn integrated(driver_version: &str) -> GpuAdapter {
        GpuAdapter {
            ordinal: 0,
            name: String::from("Fake Integrated Graphics"),
            vendor_id: 0x8086,
            device_id: 0x1234,
            driver_version: driver_version.to_owned(),
            dedicated_memory: ByteCount::new(128 * 1024 * 1024),
        }
    }

    /// Swaps the GPUs, as a hot-plug or a driver update would.
    pub fn replace(&self, adapters: Vec<GpuAdapter>) {
        lock(&self.state).adapters = adapters;
    }

    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// How many times the list was read.
    pub fn lists(&self) -> usize {
        lock(&self.state).lists
    }
}

impl GraphicsAdapters for FakeGraphicsAdapters {
    fn list(&self) -> PortResult<Vec<GpuAdapter>> {
        let mut state = lock(&self.state);
        state.lists += 1;
        match state.next_error.take() {
            Some(error) => Err(error),
            None => Ok(state.adapters.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn lists_the_script_and_fails_once_when_told() {
        let gpus = FakeGraphicsAdapters::none();
        assert_eq!(gpus.list().unwrap(), []);
        gpus.replace(vec![FakeGraphicsAdapters::integrated("1.0")]);
        gpus.fail_next(PortError::new(AppError::Internal));
        assert!(gpus.list().is_err());
        assert_eq!(gpus.list().unwrap().len(), 1);
        assert_eq!(gpus.lists(), 3);
    }
}
