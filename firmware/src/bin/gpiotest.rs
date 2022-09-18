#![no_main]
#![no_std]

#![allow(unused_imports)]

use amodem::{self as _, GlobalRollingTimer}; // global logger + panicking-behavior + memory layout

use rand_chacha::{ChaCha8Rng, rand_core::{SeedableRng, RngCore}};
use stm32g0xx_hal as hal;
use hal::{stm32, rcc::{Config, PllConfig, Prescaler, RccExt, Enable, Reset}, gpio::GpioExt, spi::{Spi, NoSck, NoMiso}, time::RateExtU32, analog::adc::AdcExt, pac::{SPI1, GPIOA, GPIOB}};
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

    SPI1::enable(&mut rcc);
    SPI1::reset(&mut rcc);
    GPIOA::enable(&mut rcc);
    GPIOB::enable(&mut rcc);

    let gpioa = board.GPIOA;
    let gpiob = board.GPIOB;
    let spi1 = board.SPI1;

    GlobalRollingTimer::init(board.TIM2);
    let timer = GlobalRollingTimer::new();

    // The configuration procedure is almost the same for master and slave.
    // For specific mode setups, follow the dedicated sections.
    // When a standard communication is to be initialized, perform these steps:

    // 1. Write proper GPIO registers: Configure GPIO for MOSI, MISO and SCK pins.

    // set alternate mode
    // SCLK: PA01 - AF0
    // MOSI: PA02 - AF0
    // MISO: PA06 - AF0
    // CSn:  PB00 - AF0
    //
    // afrl/afrh + moder
    // gpioa.afrl.modify(|_r, w| {
    //     w.afsel1().af0();
    //     w.afsel2().af0();
    //     w.afsel6().af0();
    //     w
    // });
    // gpiob.afrl.modify(|_r, w| {
    //     w.afsel0().af0();
    //     w
    // });
    gpioa.moder.modify(|_r, w| {
        w.moder1().output();
        w.moder2().output();
        w.moder6().output();
        w
    });
    gpiob.moder.modify(|_r, w| {
        w.moder0().output();
        w
    });
    gpioa.odr.modify(|_r, w| {
        w.odr1().low();
        w.odr2().low();
        w.odr6().low();
        w
    });
    gpiob.odr.modify(|_r, w| {
        w.odr0().low();
        w
    });

    // SCLK - PA1
    let start = timer.get_ticks();
    while timer.millis_since(start) < 500 { }
    gpioa.odr.modify(|_r, w| w.odr1().high());

    // MOSI - PA2
    let start = timer.get_ticks();
    while timer.millis_since(start) < 500 { }
    gpioa.odr.modify(|_r, w| w.odr2().high());

    // MISO - PA6
    let start = timer.get_ticks();
    while timer.millis_since(start) < 500 { }
    gpioa.odr.modify(|_r, w| w.odr6().high());

    // CSn - PB6
    let start = timer.get_ticks();
    while timer.millis_since(start) < 500 { }
    gpiob.odr.modify(|_r, w| w.odr0().high());

    let start = timer.get_ticks();
    while timer.millis_since(start) < 1000 { }

    Some(())
}
