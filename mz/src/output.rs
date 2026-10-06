use crate::Result;
use rodio::mixer::Mixer;

#[cfg(not(any(target_os = "android", feature = "aaudio-output")))]
pub struct Output(rodio::OutputStream);

#[cfg(not(any(target_os = "android", feature = "aaudio-output")))]
impl Output {
    pub fn open() -> Result<Self> {
        Ok(Self(rodio::OutputStreamBuilder::open_default_stream()?))
    }
    pub fn mixer(&self) -> &Mixer {
        self.0.mixer()
    }
    pub fn check(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(any(target_os = "android", feature = "aaudio-output"))]
mod aaudio;
#[cfg(any(target_os = "android", feature = "aaudio-output"))]
pub use aaudio::Output;
