use cortex_m::peripheral::NVIC;
use stm32g0xx_hal::{rcc::{Rcc, Enable}, pac::{GPIOA, GPIOB, EXTI}, exti::{ExtiExt, Event}, gpio::SignalEdge};

/// Setup GPIOs
///
/// Enables RCC for GPIOA and GPIOB. It also sets up the following
/// pins in the following modes
///
///
/// | Port  | Pin  | Role      | Mode | Add'l                       |
/// | :--   | :--  | :--       | :--  | :--                         |
/// | GPIOA | PA01 | SPI SCLK  | AF0  |                             |
/// | GPIOA | PA02 | SPI MOSI  | AF0  |                             |
/// | GPIOA | PA06 | SPI MISO  | AF0  |                             |
/// | GPIOA | PA07 | SPI RXrdy | Out  | IO2                         |
/// | GPIOA | PA11 | SPI TXrdy | Out  | IO1                         |
/// | GPIOA | PA12 | RS485 DE  | AF1  |                             |
/// | GPIOB | PB00 | SPI CSn   | AF0  | interrupt on rising edge    |
/// | GPIOB | PB06 | RS485 TXD | AF0  |                             |
/// | GPIOB | PB07 | RS485 RXD | AF0  |                             |
///
/// Also in the future I should set these up but I don't yet
///
/// | Port  | Pin  | Role      | Mode | Add'l |
/// | :--   | :--  | :--       | :--  | :--   |
/// | GPIOA | PA04 | SCL (BB)  |      |       |
/// | GPIOA | PA05 | SDA (BB)  |      |       |
/// | GPIOB | PB09 | LED1      | Out  |       |
/// | GPIOC | PC15 | LED2      | Out  |       |
#[inline]
pub fn setup_gpios(
    rcc: &mut Rcc,
    gpioa: GPIOA,
    gpiob: GPIOB,
    exti: EXTI,
) {
    GPIOA::enable(rcc);
    GPIOB::enable(rcc);

    // Setup Alternate Functions
    gpioa.afrl.modify(|_r, w| {
        w.afsel1().af0(); // SCLK
        w.afsel2().af0(); // MOSI
        w.afsel6().af0(); // MISO
        w
    });
    gpioa.afrh.modify(|_r, w| {
        w.afsel12().af1(); // DE
        w
    });
    gpiob.afrl.modify(|_r, w| {
        w.afsel0().af0(); // CSn
        w.afsel6().af0(); // TXD
        w.afsel7().af0(); // RXD
        w
    });

    // Set Mode Registers
    gpioa.moder.modify(|_r, w| {
        w.moder1().alternate(); // SCLK
        w.moder2().alternate(); // MOSI
        w.moder6().alternate(); // MISO
        w.moder7().output();    // IO2
        w.moder11().output();   // IO1
        w.moder12().alternate();   // DE
        w
    });
    gpiob.moder.modify(|_r, w| {
        w.moder0().alternate(); // CSn
        w.moder6().alternate(); // TXD
        w.moder7().alternate(); // RXD
        w
    });

    // Set Output State
    gpioa.odr.modify(|_r, w| {
        w.odr11().low(); // IO1
        w.odr7().low();  // IO2
        w
    });

    // Setup CSn EXTI interrupt
    exti.exticr1.modify(|_r, w| {
        w.exti0_7().pb();
        w
    });
    exti.listen(Event::GPIO0, SignalEdge::Rising);
}

#[inline]
pub fn unmask_csn_interrupt() {
    unsafe {
        NVIC::unmask(stm32g0xx_hal::pac::Interrupt::EXTI0_1);
    }
}

#[inline]
pub fn set_txrdy_active() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr11().high());
}

#[inline]
pub fn set_txrdy_inactive() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr11().low());
}

#[inline]
pub fn set_rxrdy_active() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr7().high());
}

#[inline]
pub fn set_rxrdy_inactive() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr7().low());
}
