#![no_main]
#![no_std]

#![allow(unused_imports)]

use amodem::{self as _, GlobalRollingTimer}; // global logger + panicking-behavior + memory layout

use rand_chacha::{ChaCha8Rng, rand_core::{SeedableRng, RngCore}};
use stm32g0xx_hal as hal;
use hal::{stm32, rcc::{Config, PllConfig, Prescaler, RccExt, Enable, Reset}, gpio::GpioExt, spi::{Spi, NoSck, NoMiso}, time::RateExtU32, analog::adc::AdcExt, pac::{SPI1, GPIOA, GPIOB}};
use groundhog::RollingTimer;
use hal::interrupt;

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
    gpioa.afrl.modify(|_r, w| {
        w.afsel1().af0();
        w.afsel2().af0();
        w.afsel6().af0();
        w
    });
    gpiob.afrl.modify(|_r, w| {
        w.afsel0().af0();
        w
    });
    gpioa.moder.modify(|_r, w| {
        w.moder1().alternate();
        w.moder2().alternate();
        w.moder6().alternate();
        w.moder7().output();
        w
    });
    gpiob.moder.modify(|_r, w| {
        w.moder0().alternate();
        w
    });

    // 2. Write to the SPI_CR1 register:
    //     * XXX - Configure the serial clock baud rate using the BR[2:0] bits (Note: 4).
    //     * Configure the CPOL and CPHA bits combination to define one of the
    //         four relationships between the data transfer and the serial clock (CPHA must be cleared in NSSP mode).
    //     * Select simplex or half-duplex mode by configuring RXONLY or BIDIMODE and BIDIOE (RXONLY and BIDIMODE can't be set at the same time).
    //     * Configure the LSBFIRST bit to define the frame format
    //     * Configure the CRCL and CRCEN bits if CRC is needed (while SCK clock signal is at idle state).
    //     * Configure SSM and SSI
    //     * Configure the MSTR bit (in multimaster NSS configuration, avoid conflict state on NSS if master is configured to prevent MODF error).
    spi1.cr1.modify(|_r, w| {
        w.bidimode().unidirectional();
        // w.bidioe();
        w.crcen().disabled();
        // w.crcnext();
        // w.crcl();
        w.rxonly().full_duplex();
        w.ssm().disabled();
        w.ssi().slave_selected(); // ?
        w.lsbfirst().msbfirst();
        // w.spe();
        w.br().div2();
        w.mstr().slave();
        w.cpol().idle_low();
        w.cpha().first_edge();
        w
    });

    // 3. Write to SPI_CR2 register:
    //     * Configure the DS[3:0] bits to select the data length for the transfer.
    //     * Configure SSOE (Notes: 1 & 2 & 3).
    //     * Set the FRF bit if the TI protocol is required (keep NSSP bit cleared in TI mode).
    //     * Set the NSSP bit if the NSS pulse mode between two data units is required (keep CHPA and TI bits cleared in NSSP mode).
    //     * Configure the FRXTH bit. The RXFIFO threshold must be aligned to the read access size for the SPIx_DR register.
    //     * Initialize LDMA_TX and LDMA_RX bits if DMA is used in packed mode.
    spi1.cr2.modify(|_r, w| {
        // w.ldma_tx();
        // w.ldma_rx();
        w.frxth().quarter(); // 8-bit
        w.ds().eight_bit();
        w.txeie().masked();
        w.rxneie().masked();
        w.errie().masked();
        w.frf().motorola();
        w.nssp().no_pulse();
        w.ssoe().disabled();
        w.txdmaen().disabled();
        w.rxdmaen().disabled();
        w
    });

    // 4. Write to SPI_CRCPR register: Configure the CRC polynomial if needed.
    // 5. Write proper DMA registers: Configure DMA streams dedicated for SPI Tx and Rx in DMA registers if the DMA streams are used.

    // Notes:

    // * (Note 1) Step is not required in slave mode.
    // * (Note 2) Step is not required in TI mode.
    // * (Note 3) Step is not required in NSSP mode.
    // * (Note 4) The step is not required in slave mode except slave working at TI mode

    gpioa.odr.modify(|_r, w| w.odr7().low());
    let start = timer.get_ticks();
    while timer.millis_since(start) < 100 { }

    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    // 4001300C
    defmt::println!("dr8b: {:08X}", dr8b as usize as u32);
    spi1.cr1.modify(|_r, w| w.spe().enabled());

    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    let sr = spi1.sr.read().bits();
    defmt::println!("SRA: {:04X}", sr);

    unsafe {
        dr8b.write_volatile(0x42);
        dr8b.write_volatile(0x69);
    }

    let sr = spi1.sr.read().bits();
    defmt::println!("SRB: {:04X}", sr);

    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    defmt::println!("Waiting...");
    gpioa.odr.modify(|_r, w| w.odr7().high());

    let mut rx = [0xFF; 2];
    rx.iter_mut().for_each(|b| {
        while spi1.sr.read().rxne().is_empty() { }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        *b = unsafe { dr8b.read_volatile() };
    });

    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    defmt::println!("Got data: {:?}", &rx);
    let sr = spi1.sr.read().bits();
    defmt::println!("SRC: {:04X}", sr);

    gpioa.odr.modify(|_r, w| w.odr7().low());
    let start = timer.get_ticks();
    while timer.millis_since(start) < 100 { }

    // let x = stm32g0xx_hal::pac::Interrupt::EXTI0_1;

    Some(())
}

#[interrupt]
fn EXTI0_1() {

}

#[interrupt]
fn SPI1() {

}
