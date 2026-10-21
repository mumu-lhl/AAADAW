use aaadaw_core::MasterMix;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Control-thread handle for coherent stereo Master gain updates.
#[derive(Clone, Debug)]
pub struct MasterMixController(Arc<AtomicU64>);

impl MasterMixController {
    pub fn set_mix(&self, mix: MasterMix) {
        self.0.store(pack(mix.channel_gains()), Ordering::Relaxed);
    }
}

fn pack(gains: [f32; 2]) -> u64 {
    u64::from(gains[0].to_bits()) | (u64::from(gains[1].to_bits()) << 32)
}

fn coefficients(gains: [f32; 2]) -> super::GainCoefficients {
    super::GainCoefficients {
        left: gains[0],
        right: gains[1],
        stereo_left: gains[0],
        stereo_right: gains[1],
    }
}

pub(super) struct MasterOutputMix {
    controller: MasterMixController,
    ramp: super::GainRamp,
}

impl MasterOutputMix {
    pub(super) fn new(mix: MasterMix, sample_rate: u32) -> Self {
        let gains = mix.channel_gains();
        Self {
            controller: MasterMixController(Arc::new(AtomicU64::new(pack(gains)))),
            ramp: super::GainRamp::new(coefficients(gains), (sample_rate / 200).max(1) as usize),
        }
    }

    pub(super) fn controller(&self) -> MasterMixController {
        self.controller.clone()
    }

    pub(super) fn process(&mut self, output: &mut [[f32; 2]]) {
        let value = self.controller.0.load(Ordering::Relaxed);
        self.ramp.retarget(coefficients([
            f32::from_bits(value as u32),
            f32::from_bits((value >> 32) as u32),
        ]));
        for frame in output {
            let gains = self.ramp.next_frame();
            frame[0] *= gains.left;
            frame[1] *= gains.right;
        }
    }
}
