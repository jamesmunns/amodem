#![no_main]
#![no_std]

use defmt_rtt as _; // global logger

use nrf52840_hal as _; // memory layout

use panic_probe as _;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}

// defmt-test 0.3.0 has the limitation that this `#[tests]` attribute can only be used
// once within a crate. the module can be in any file but there can only be at most
// one `#[tests]` module in this library crate
#[cfg(test)]
#[defmt_test::tests]
mod unit_tests {
    use defmt::assert;

    #[test]
    fn it_works() {
        assert!(true)
    }
}

use groundhog::RollingTimer;
use nrf52840_hal::{pac::timer0::RegisterBlock as RegBlock0, timer::Instance};
use core::sync::atomic::{AtomicPtr, Ordering};

static TIMER_PTR: AtomicPtr<RegBlock0> = AtomicPtr::new(core::ptr::null_mut());

/// A global rolling timer
///
/// This must be initialized with a timer (like `TIMER0`) once,
/// on startup, before valid timer values will be returned. Until then,
/// a timer value of 0 ticks will always be returned.
///
/// At the moment, this is limited to a 32-bit 1MHz timer, which has a
/// maximum observable time delta of 71m34s.
pub struct GlobalRollingTimer;

impl GlobalRollingTimer {
    pub const fn new() -> Self {
        Self
    }

    pub fn init<T: Instance>(timer: T) {
        timer.set_periodic();
        timer.timer_start(0xFFFF_FFFFu32);
        let t0 = timer.as_timer0();

        let old_ptr = TIMER_PTR.swap(t0 as *const _ as *mut _, Ordering::SeqCst);

        debug_assert!(old_ptr == core::ptr::null_mut());
    }
}

impl RollingTimer for GlobalRollingTimer {
    type Tick = u32;
    const TICKS_PER_SECOND: u32 = 1_000_000;

    fn get_ticks(&self) -> u32 {
        if let Some(t0) = unsafe { TIMER_PTR.load(Ordering::SeqCst).as_ref() } {
            t0.tasks_capture[1].write(|w| unsafe { w.bits(1) });
            t0.cc[1].read().bits()
        } else {
            0
        }
    }

    fn is_initialized(&self) -> bool {
        TIMER_PTR.load(Ordering::SeqCst) != core::ptr::null_mut()
    }
}
