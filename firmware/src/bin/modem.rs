#![no_main]
#![no_std]


use amodem::{
    self as _,
    modem::{
        setup_sys_clocks,
        setup_rolling_timer,
        gpios::setup_gpios,
        spi::setup_spi,
        pipes::PIPES
    },
};

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

    loop {
        PIPES.idle_step();
    }
}


#[interrupt]
fn EXTI0_1() {
    todo!("EXTI INTERRUPT!");
}

#[interrupt]
fn SPI1() {
    todo!("SPI1 INTERRUPT");
}

#[interrupt]
fn USART1() {
    todo!("USART1 INTERRUPT");
}
