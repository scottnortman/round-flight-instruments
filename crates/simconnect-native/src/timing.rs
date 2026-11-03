//! Process-scoped Windows wait resolution, held only while servicing a link.
//! This improves wait scheduling; it is not a real-time or clock-accuracy claim.
use std::io;
#[link(name = "winmm")]
unsafe extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
    fn timeEndPeriod(period: u32) -> u32;
}
pub struct TimerResolution(());
impl TimerResolution {
    pub fn one_ms() -> io::Result<Self> {
        // SAFETY: scalar documented WinMM call; every success has a matching Drop.
        if unsafe { timeBeginPeriod(1) } != 0 {
            return Err(io::Error::other("Windows rejected 1 ms wait resolution"));
        }
        Ok(Self(()))
    }
}
impl Drop for TimerResolution {
    fn drop(&mut self) {
        // SAFETY: balances this instance's successful timeBeginPeriod(1).
        unsafe {
            timeEndPeriod(1);
        }
    }
}
