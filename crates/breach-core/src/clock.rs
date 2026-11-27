//! Elapsed time that also compiles for wasm32, where the caller times the call and this reports zero.

#[cfg(not(target_arch = "wasm32"))]
pub struct Stopwatch(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl Stopwatch {
    pub fn start() -> Self {
        Stopwatch(std::time::Instant::now())
    }
    pub fn us(&self) -> u32 {
        self.0.elapsed().as_micros().min(u32::MAX as u128) as u32
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Stopwatch;

#[cfg(target_arch = "wasm32")]
impl Stopwatch {
    pub fn start() -> Self {
        Stopwatch
    }
    pub fn us(&self) -> u32 {
        0
    }
}
