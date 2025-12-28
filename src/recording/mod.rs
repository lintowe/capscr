mod gif_encoder;

pub use gif_encoder::GifRecorder;

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Processing,
}

#[derive(Debug, Clone)]
pub struct RecordingSettings {
    pub fps: u32,
    pub max_duration: Duration,
    pub quality: u8,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            fps: 15,
            max_duration: Duration::from_secs(30),
            quality: 80,
        }
    }
}
