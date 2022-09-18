#![no_main]
#![no_std]

#![allow(unused_imports)]

use amodem::{self as _, GlobalRollingTimer}; // global logger + panicking-behavior + memory layout

use rand_chacha::{ChaCha8Rng, rand_core::{SeedableRng, RngCore}};
use stm32g0xx_hal as hal;
use hal::{stm32, rcc::{Config, PllConfig, Prescaler, RccExt}, gpio::GpioExt, spi::{Spi, NoSck, NoMiso}, time::RateExtU32, analog::adc::AdcExt};
use groundhog::RollingTimer;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::println!("Hello, world!");

    if let Some(_) = imain() {
        defmt::println!("OK");
    } else {
        defmt::println!("ERR");
    }

    amodem::exit()
}


fn imain() -> Option<()> {
    let board = stm32::Peripherals::take()?;
    let _core = stm32::CorePeripherals::take()?;

    // Configure clocks
    let config = Config::pll()
        .pll_cfg(PllConfig::with_hsi(1, 8, 2))
        .ahb_psc(Prescaler::NotDivided)
        .apb_psc(Prescaler::NotDivided);
    let mut rcc = board.RCC.freeze(config);

    let _gpioa = board.GPIOA.split(&mut rcc);
    let _gpiob = board.GPIOB.split(&mut rcc);


    Some(())
}
