#![no_main]
#![no_std]

#![allow(unused_imports, dead_code, unused_variables)]

use core::{sync::atomic::{AtomicU8, AtomicU16, Ordering}, cell::UnsafeCell, mem::MaybeUninit};

use amodem::{self as _, GlobalRollingTimer, modem::{setup_sys_clocks, setup_rolling_timer, gpios::setup_gpios, spi::setup_spi}}; // global logger + panicking-behavior + memory layout

use bbqueue_spicy::{BBBuffer, framed::{FrameGrantW, FrameGrantR}};
use cortex_m::peripheral::NVIC;
use rand_chacha::{ChaCha8Rng, rand_core::{SeedableRng, RngCore}};
use stm32g0xx_hal as hal;
use hal::{stm32, rcc::{Config, PllConfig, Prescaler, RccExt, Enable, Reset}, gpio::GpioExt, spi::{Spi, NoSck, NoMiso}, time::RateExtU32, analog::adc::AdcExt, pac::{SPI1, GPIOA, GPIOB, DMA}, exti::{ExtiExt, Event}, dma::{DmaExt, C1, Channel, WordSize, Direction}};
use groundhog::RollingTimer;
use hal::interrupt;

static SPI_MODE: AtomicU8 = AtomicU8::new(MODE_IDLE);

const MODE_IDLE: u8 = 0b000_00000;
const MODE_RELOAD: u8 = 0b000_00001;


// TODO: This should probably be an enum or something. Be careful when updating.
//
const MODE_MASK: u8 = 0b111_00000;

const MODE_LONG_PKT_READ: u8 = 0b001_00000;
const MODE_LONG_PKT_WRITE: u8 = 0b010_00000;
const MODE_SHORT_REG_READ: u8 = 0b011_00000;
const MODE_SHORT_REG_WRITE: u8 = 0b100_00000;
const MODE_INVALID_WAIT: u8 = 0b111_00000;
//
// ENDTODO

const ONE_ATOMIC: AtomicU16 = AtomicU16::new(0xACAB);
static REGS: [AtomicU16; 32] = [ONE_ATOMIC; 32];

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


unsafe impl Sync for DmaBox { }

struct DmaBox {
    ch1: UnsafeCell<MaybeUninit<C1>>,
    buf: UnsafeCell<[u8; 256]>,
}

static DMA_BOX: DmaBox = DmaBox {
    buf: UnsafeCell::new([0u8; 256]),
    ch1: UnsafeCell::new(MaybeUninit::uninit()),
};

fn imain() -> Option<()> {
    let board = stm32::Peripherals::take()?;

    // Configure clocks
    let mut rcc = setup_sys_clocks(board.RCC);
    setup_rolling_timer(board.TIM2);
    let dma = board.DMA.split(&mut rcc, board.DMAMUX);
    setup_gpios(&mut rcc, board.GPIOA, board.GPIOB, board.EXTI);
    setup_spi(&mut rcc, board.SPI1, dma.C1, dma.C2);


    let timer = GlobalRollingTimer::new();

    Some(())
}


#[interrupt]
fn EXTI0_1() {
    todo!()
}

#[interrupt]
fn SPI1() {
    todo!()
}
