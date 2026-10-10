/// Validated controls for the stereo Master output, independent of track identities.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MasterMix {
    volume_db: f32,
    pan: f32,
}

impl MasterMix {
    /// Creates finite gain and a stereo balance in the inclusive range -1..=1.
    pub fn new(volume_db: f32, pan: f32) -> Result<Self, crate::ActionError> {
        if !volume_db.is_finite() || !10.0_f32.powf(volume_db / 20.0).is_finite() {
            return Err(crate::ActionError::InvalidVolumeDb);
        }
        if !pan.is_finite() || !(-1.0..=1.0).contains(&pan) {
            return Err(crate::ActionError::InvalidPan);
        }
        Ok(Self { volume_db, pan })
    }

    pub fn volume_db(self) -> f32 {
        self.volume_db
    }

    pub fn pan(self) -> f32 {
        self.pan
    }

    /// Linear 0 dB stereo balance; Master receives an already mixed stereo signal.
    pub fn channel_gains(self) -> [f32; 2] {
        let gain = 10.0_f32.powf(self.volume_db / 20.0);
        [
            gain * (1.0 - self.pan.max(0.0)),
            gain * (1.0 + self.pan.min(0.0)),
        ]
    }
}
