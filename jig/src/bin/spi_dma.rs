#![no_main]
#![no_std]

use cortex_m::singleton;
use groundhog::RollingTimer;
use jig::{self as _, GlobalRollingTimer}; // global logger + panicking-behavior + memory layout
use nrf52840_hal::{self, Clocks, clocks::{ExternalOscillator, Internal, LfOscStopped}, spim::MODE_0, Spim, gpio::Level};
use nrf52840_hal::gpio::p0::Parts as P0Parts;
use nrf52840_hal::gpio::p1::Parts as P1Parts;
use nrf52840_hal::spim::Pins as SpimPins;
use nrf52840_hal::spim::Frequency as SpimFreq;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::println!("Hello, world!");

    let _ = imain();

    jig::exit()
}

fn imain() -> Option<()> {
    let device = nrf52840_hal::pac::Peripherals::take()?;

    // Setup clocks early in the process. We need this for USB later
    let clocks = Clocks::new(device.CLOCK);
    let clocks = clocks.enable_ext_hfosc();
    let clocks = singleton!(: Clocks<ExternalOscillator, Internal, LfOscStopped> = clocks)?;

    // Create GPIO ports for pin-mapping
    let port0 = P0Parts::new(device.P0);
    let port1 = P1Parts::new(device.P1);

    GlobalRollingTimer::init(device.TIMER0);
    let timer = GlobalRollingTimer::new();

    // Set up GPIOs
    // IO1: PA11 (also PA09)
    //     -> P1.01
    // IO2: PA07
    //     -> P1.02
    let io1 = port1.p1_01.into_floating_input();
    let io2 = port1.p1_02.into_floating_input();

    // SCLK: PA01
    //     -> P1.04
    // MOSI: PA02
    //     -> P1.03
    // MISO: PA06
    //     -> P1.05
    // CSn:  PB00 (also PB01,02, PA08)
    //     -> P1.06

    // Set up Spim
    let mut csn = port1.p1_06.into_push_pull_output(Level::High).degrade();
    let sck = port1.p1_04.into_push_pull_output(Level::Low).degrade();
    let mosi = port1.p1_03.into_push_pull_output(Level::Low).degrade();
    let miso = port1.p1_05.into_floating_input().degrade();
    let mut spi = Spim::new(
        device.SPIM2,
        SpimPins {
            sck,
            miso: Some(miso),
            mosi: Some(mosi),
        },
        SpimFreq::M1,
        MODE_0,
        0,
    );


    // const MODE_LONG_PKT_READ: u8 = 0b001_00000;
    // const MODE_LONG_PKT_WRITE: u8 = 0b010_00000;
    // const MODE_SHORT_REG_READ: u8 = 0b011_00000;
    // const MODE_SHORT_REG_WRITE: u8 = 0b100_00000;
    // const MODE_INVALID_WAIT: u8 = 0b111_00000;

    let mut bufout = [0x44u8; 128];

    // Read
    {
        let start = timer.get_ticks();

        bufout[0] = 0b001_00000; // READ
        // bufout[0] = 0b010_00000; // WRITE

        while timer.millis_since(start) < 250 { }

        match spi.transfer(&mut csn, &mut bufout) {
            Ok(_) => {
                defmt::println!("OK");
                defmt::println!("{:02X}", &bufout);
            },
            Err(_) => {
                defmt::println!("ERR");
            },
        }
    }

    Some(())
}
