#![no_main]
#![no_std]


use amodem::{
    self as _,
    modem::{
        setup_sys_clocks,
        setup_rolling_timer,
        gpios::setup_gpios,
        spi::{setup_spi, spi_int_unmask, exti_isr, spi_isr},
        pipes::PIPES
    },
};

use cortex_m::peripheral::NVIC;
use stm32g0xx_hal as hal;
use hal::stm32;
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

    // Configure clocks
    let mut rcc = setup_sys_clocks(board.RCC);
    setup_rolling_timer(board.TIM2);
    setup_gpios(&mut rcc, board.GPIOA, board.GPIOB, board.EXTI);
    setup_spi(&mut rcc, board.SPI1);
    unsafe {
        PIPES.init(&mut rcc, board.DMA, board.DMAMUX);
    }

    // Actually enable the SPI interrupt
    spi_int_unmask();

    unsafe {
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::EXTI0_1);
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::SPI1);
    }

    loop {
        PIPES.idle_step();
    }
}


#[interrupt]
fn EXTI0_1() {
    exti_isr();
}

#[interrupt]
fn SPI1() {
    spi_isr();
}

#[interrupt]
fn USART1() {
    todo!("USART1 INTERRUPT");
}
