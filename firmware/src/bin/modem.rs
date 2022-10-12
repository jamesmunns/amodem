#![no_main]
#![no_std]


use core::sync::atomic::{AtomicBool, Ordering};

use amodem::{
    self as _,
    modem::{
        setup_sys_clocks,
        setup_rolling_timer,
        gpios::setup_gpios,
        spi::{setup_spi, spi_int_unmask, exti_isr, spi_isr},
        pipes::{PIPES, self}, rs485::{setup_rs485, rs485_isr}
    }, GlobalRollingTimer,
};

use cortex_m::peripheral::NVIC;
use groundhog::RollingTimer;
use stm32g0xx_hal as hal;
use hal::{stm32, pac::USART1};
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
    setup_gpios(
        &mut rcc,
        board.GPIOA,
        board.GPIOB,
        board.GPIOC,
        board.EXTI
    );
    setup_spi(&mut rcc, board.SPI1);
    setup_rs485(&mut rcc, board.USART1);

    unsafe {
        PIPES.init(&mut rcc, board.DMA, board.DMAMUX);
    }

    unsafe {
        // TODO: Priorities. Probably in this order, highest to lowest
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::EXTI0_1);
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::SPI1);
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::USART1);
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::DMA_CHANNEL2_3);
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::DMA_CHANNEL4_5_6_7);
    }



    loop {
        PIPES.idle_step();
        // let usart1 = unsafe { &*USART1::PTR };
        // while usart1.isr.read().rxne().bit_is_set() {
        //     let data = usart1.rdr.read().rdr().bits();
        //     defmt::println!("IDLE Got {:04X}", data);
        // }
    }
}

#[interrupt]
fn DMA_CHANNEL2_3() {
    // Note: only channel 3 interrupts are used at the moment, for signalling
    // rs-485 receive is complete
    rs485_isr();
    defmt::println!("DMA ISR 23")
}

#[interrupt]
fn DMA_CHANNEL4_5_6_7() {
    // Note: only channel 4 interrupts are used at the moment, for signalling
    // rs-485 tx is complete
    rs485_isr();
    defmt::println!("DMA ISR 4567")
}

#[interrupt]
fn EXTI0_1() {
    exti_isr();
}

#[interrupt]
fn SPI1() {
    spi_isr();
}

static ONESHOT: AtomicBool = AtomicBool::new(false);

#[interrupt]
fn USART1() {
    rs485_isr();
}
