#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::{sync::atomic::{AtomicU8, AtomicU16, Ordering}, cell::UnsafeCell, mem::MaybeUninit};

use amodem::{self as _, GlobalRollingTimer}; // global logger + panicking-behavior + memory layout

use cortex_m::peripheral::NVIC;
use rand_chacha::{ChaCha8Rng, rand_core::{SeedableRng, RngCore}};
use stm32g0xx_hal as hal;
use hal::{stm32, rcc::{Config, PllConfig, Prescaler, RccExt, Enable, Reset}, gpio::GpioExt, spi::{Spi, NoSck, NoMiso}, time::RateExtU32, analog::adc::AdcExt, pac::{SPI1, GPIOA, GPIOB, DMA, USART1}, exti::{ExtiExt, Event}, dma::{DmaExt, C1, Channel, WordSize, Direction}};
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
    // let core = stm32::CorePeripherals::take()?;

    // Configure clocks
    let config = Config::pll()
        .pll_cfg(PllConfig::with_hsi(1, 8, 2))
        .ahb_psc(Prescaler::NotDivided)
        .apb_psc(Prescaler::NotDivided);
    let mut rcc = board.RCC.freeze(config);

    SPI1::enable(&mut rcc);
    SPI1::reset(&mut rcc);
    USART1::enable(&mut rcc);
    GPIOA::enable(&mut rcc);
    GPIOB::enable(&mut rcc);

    let gpioa = board.GPIOA;
    let gpiob = board.GPIOB;
    let spi1 = board.SPI1;
    let usart1 = board.USART1;

    GlobalRollingTimer::init(board.TIM2);
    let timer = GlobalRollingTimer::new();


    // USART1 -------------------------
    // RXD: PB07 (also PB08)        - USART1RX          - AF0
    // TXD: PB06 (also PB03,04,05)  - USART1TX          - AF0
    // DE:  PA12 (also PA10)        - USART1_RTS_DE_CK  - AF1


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
    gpioa.moder.modify(|_r, w| {
        w.moder1().alternate();  // SCLK
        w.moder2().alternate();  // MOSI
        w.moder6().alternate();  // MISO
        w.moder7().output();     // IO
        w.moder12().alternate(); // DE
        // w.moder12().output(); // DE
        w
    });
    gpiob.moder.modify(|_r, w| {
        w.moder0().alternate(); // CSn
        w.moder6().alternate(); // TXD
        w.moder7().alternate(); // RXD
        w
    });
    // gpioa.odr.modify(|_r, w| w.odr12().high());

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
        // w.ssi();
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


    //                                                           //
    // SPI MAIN
    //                                                           //

    // let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    // spi1.cr1.modify(|_r, w| w.spe().enabled());

    // // DMA
    // let dma = board.DMA.split(&mut rcc, board.DMAMUX);
    // let ch1: C1 = dma.ch1;

    // unsafe { DMA_BOX.ch1.get().write(MaybeUninit::new(ch1)); }
    // // End DMA

    // board.EXTI.exticr1.modify(|_r, w| {
    //     w.exti0_7().pb();
    //     w
    // });
    // board.EXTI.listen(Event::GPIO0, hal::gpio::SignalEdge::Rising);

    // // TODO: Interrupt priority
    // //
    // // I definitely want EXTI to be higher than SPI1, not sure wrt USART/DMA ints.

    // // Enable EXTI int
    unsafe {
        // NVIC::unmask(stm32g0xx_hal::pac::Interrupt::EXTI0_1);
        // NVIC::unmask(stm32g0xx_hal::pac::Interrupt::SPI1);
        // NVIC::unmask(stm32g0xx_hal::pac::Interrupt::USART1);
    }

    // loop {
    //     // Clear ERR flags
    //     // ? TODO

    //     // Push two dummy bytes
    //     unsafe {
    //         dr8b.write_volatile(0x12);
    //         dr8b.write_volatile(0x34);
    //     }

    //     // Enable RXNE Int
    //     spi1.cr2.modify(|_r, w| w.rxneie().not_masked());

    //     // Clear Busy Flag
    //     set_not_busy();

    //     defmt::println!("Waiting!");

    //     // WFI until EXTI
    //     while SPI_MODE.load(Ordering::Relaxed) != MODE_RELOAD {
    //         cortex_m::asm::nop();
    //     }

    //     defmt::println!("DONG");

    //     // Cleanup? Handle things NOT in interrupt context?
    //     SPI_MODE.store(MODE_IDLE, Ordering::Relaxed);
    // }


    //                                                           //
    // RS485 MAIN
    //                                                           //

    usart1.cr1.modify(|_r, w| {
        w.rxffie().disabled();
        w.txfeie().disabled();
        w.fifoen().enabled();
        w.m1().m0();
        w.eobie().disabled();
        w.rtoie().disabled();
        w.deat().variant(8); // assertion: one bit time. TODO: check xcvr timing
        w.dedt().variant(4); // deassertn: one bit time. TODO: check xcvr timing
        w.over8().oversampling8();
        w.cmie().enabled();
        w.mme().enabled();
        w.m0().bit9();
        w.wake().address();
        w.pce().disabled();
        // w.ps();
        w.peie().disabled();
        w.txeie().disabled(); // This is txfnfie
        w.tcie().disabled();
        w.rxneie().disabled(); // this is rxfneie
        w.idleie().disabled();
        w.te().enabled();
        w.re().enabled();
        w.uesm().disabled();
        w.ue().disabled(); // We will enable this after config is done
        w
    });

    usart1.cr2.modify(|_r, w| {
        w.add().variant(0x40); // TODO: variable address match
        w.rtoen().disabled(); // TODO
        // w.abrmod();
        w.abren().disabled();
        w.msbfirst().lsb();
        w.datainv().positive();
        w.txinv().standard();
        w.rxinv().standard();
        w.swap().standard();
        w.linen().disabled();
        w.stop().stop1();
        w.clken().disabled();
        // w.cpol();
        // w.cpha();
        // w.lbcl();
        w.lbdie().disabled();
        // w.lbdl();
        w.addm7().bit7();
        // w.dis_nss();
        // w.slven();
        w
    });

    usart1.cr3.modify(|_r, w| {
        w.txftcfg();
        w.rxftie().disabled();
        w.rxftcfg();
        w.tcbgtie().disabled();
        w.txftie().disabled();
        w.wufie().disabled();
        // w.wus();
        // w.scarcnt();
        w.dep().high();
        w.dem().enabled();
        w.ddre().disabled();
        w.ovrdis().disabled();
        w.onebit().sample3();
        w.ctsie().disabled();
        w.ctse().disabled();
        w.rtse().disabled();
        w.dmat().disabled();
        w.dmar().disabled();
        w.scen().disabled();
        w.nack().disabled();
        w.hdsel().not_selected();
        // w.irlp();
        w.iren().disabled();
        w.eie().disabled();
        w
    });

    usart1.brr.modify(|_r, w| {
        w.brr().variant(0x0010);
        w
    });

    usart1.rtor.modify(|_r, w| {
        w
    });

    usart1.cr1.modify(|_r, w| w.ue().enabled());

    let start = timer.get_ticks();
    while timer.millis_since(start) <= 100 {}

    usart1.cr1.modify(|_r, w| {
        w.te().enabled();
        w.re().enabled();
        w
    });

    defmt::println!("Write once...");
    for _ in 0..1 {
        usart1.tdr.write(|w| unsafe { w.bits(0x0140) });

        let len_rxcap = 512u16;
        let lenb_rxcap = len_rxcap.to_le_bytes();
        lenb_rxcap.iter().for_each(|b| usart1.tdr.write(|w| w.tdr().bits((*b) as u16)));
        let len_txcap = 64u16;
        let lenb_txcap = len_txcap.to_le_bytes();
        lenb_txcap.iter().for_each(|b| usart1.tdr.write(|w| w.tdr().bits((*b) as u16)));
        while usart1.isr.read().tc().bit_is_clear() { }
        let start = timer.get_ticks();
        while timer.micros_since(start) <= 200 {}
        for _ in 0..64 {
            usart1.tdr.write(|w| w.tdr().bits(0x00AF));
            while usart1.isr.read().tc().bit_is_clear() { }
        }

        // Force mute mode
        usart1.tdr.write(|w| w.tdr().bits(0x01FF));
        while usart1.isr.read().tc().bit_is_clear() { }

        while timer.micros_since(start) <= 10_000 { }
    }

    // let start = timer.get_ticks();
    // while timer.ticks_since(start) <= 20 {}

    // while usart1.isr.read().tc().bit_is_clear() { }
    // let start = timer.get_ticks();
    // while timer.ticks_since(start) <= 20 {}

    // while usart1.isr.read().tc().bit_is_clear() { }
    // let start = timer.get_ticks();
    // while timer.ticks_since(start) <= 20 {}

    // let start = timer.get_ticks();
    // while timer.millis_since(start) <= 100 {}

    // defmt::println!("Write twice...");
    // usart1.tdr.write(|w| unsafe { w.bits(0x0139) });
    // usart1.tdr.write(|w| unsafe { w.bits(0x0023) });
    // usart1.tdr.write(|w| unsafe { w.bits(0x0045) });
    // while usart1.isr.read().tc().bit_is_clear() { }

    defmt::println!("DONE!");

    Some(())

    // defmt::println!("Waiting for data...");
    // usart1.rqr.write(|w| w.mmrq().set_bit());

    // use hal::pac::Interrupt::USART1

    // loop {
    //     if usart1.isr.read().rxne().bit_is_set() {
    //         let data = usart1.rdr.read().rdr().bits();
    //         defmt::println!("Got {:04X}", data);
    //     }
    // }
}

fn set_busy() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr7().high());
}

fn set_not_busy() {
    let gpioa = unsafe { &*GPIOA::PTR };
    gpioa.odr.modify(|_r, w| w.odr7().low());
}

#[interrupt]
fn USART1() {
    defmt::panic!("USART DING");
}

#[interrupt]
fn EXTI0_1() {
    let val = SPI_MODE.load(Ordering::Relaxed);
    let mode = val & MODE_MASK;
    let low = val & !MODE_MASK;

    let exti = unsafe { &*hal::pac::EXTI::PTR };
    let spi1 = unsafe { &*SPI1::PTR };
    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();

    exti.rpr1.modify(|_r, w| w.rpif0().set_bit());

    // TODO: Probably disable SPI via SPE, let main re-enable it

    match mode {
        MODE_SHORT_REG_READ => {
            // We have already sent the value, and we don't care about
            // the data sent to us here. The FIFO will be drained below.
        },
        MODE_SHORT_REG_WRITE => {
            // We need to get the next two bytes out of the FIFO to store to the
            // proper register.
            let pop_byte = || {
                if !spi1.sr.read().rxne().is_empty() {
                    Some(unsafe { dr8b.read_volatile() })
                } else {
                    None
                }
            };

            if let (Some(a), Some(b)) = (pop_byte(), pop_byte()) {
                let val = u16::from_le_bytes([a, b]);
                // defmt::println!("Wrote {:04X} to {:?}", val, low);
                REGS[low as usize].store(val, Ordering::Relaxed);
            }
        },
        MODE_LONG_PKT_READ => {
            spi1.cr2.modify(|_r, w| w.txdmaen().disabled());
            let ch1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };
            let dma = unsafe { &*DMA::PTR };
            let remain = dma.ch1().ndtr.read().ndt().bits();
            defmt::println!("TX Remain: {:?}", remain);
            ch1.disable();
        },
        MODE_LONG_PKT_WRITE => {
            spi1.cr2.modify(|_r, w| w.rxdmaen().disabled());
            let ch1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };
            let dma = unsafe { &*DMA::PTR };
            let remain = dma.ch1().ndtr.read().ndt().bits();
            let slice = unsafe { &*DMA_BOX.buf.get() };
            slice.chunks(16).for_each(|c| {
                defmt::println!("{:02X}", c);
            });
            defmt::println!("RX Remain: {:?}", remain);
            ch1.disable();
        },
        _ => {
            // Huh, that was weird.
        },
    }

    while !spi1.sr.read().rxne().is_empty() {
        let _ = unsafe { dr8b.read_volatile() };
    }

    // TODO: Drain TX FIFO?
    SPI_MODE.store(MODE_RELOAD, Ordering::Relaxed);
    set_not_busy();
}

#[interrupt]
fn SPI1() {
    // Set Busy Pin
    set_busy();

    let spi1 = unsafe { &*SPI1::PTR };
    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    let dr16b: *mut u16 = spi1.dr.as_ptr().cast();

    // Disable RXNE interrupt
    spi1.cr2.modify(|_r, w| w.rxneie().masked());

    // defmt::assert!(!spi1.sr.read().rxne().is_empty());
    let mode = SPI_MODE.load(Ordering::Relaxed);
    if mode != MODE_IDLE {
        return;
    }

    // Read first FIFO byte
    let fbyte = unsafe { dr8b.read_volatile() };

    let mode = fbyte & MODE_MASK;
    let low = fbyte & !MODE_MASK;

    match mode {
        MODE_SHORT_REG_READ => {
            // Push two bytes into the FIFO.
            let val = REGS[low as usize].load(Ordering::Relaxed);
            unsafe {
                dr16b.write_volatile(val);
            };
            // Wait for EXTI to complete
            SPI_MODE.store(MODE_SHORT_REG_READ, Ordering::Relaxed);
        },
        MODE_SHORT_REG_WRITE => {
            // Nothing else to do, just wait for EXTI.
            SPI_MODE.store(fbyte, Ordering::Relaxed);
        },
        MODE_LONG_PKT_READ => {
            // Write len
            unsafe {
                dr16b.write_volatile(256);
            };

            let ch1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };

            ch1.set_word_size(WordSize::BITS8);
            ch1.set_memory_address(DMA_BOX.buf.get() as usize as u32, true);
            ch1.set_peripheral_address(dr8b as usize as u32, false);
            ch1.set_transfer_length(256);

            ch1.set_direction(Direction::FromMemory);
            ch1.select_peripheral(hal::dmamux::DmaMuxIndex::SPI1_TX);
            ch1.enable();
            spi1.cr2.modify(|_r, w| w.txdmaen().enabled());
            SPI_MODE.store(MODE_LONG_PKT_READ, Ordering::Relaxed);
        },
        MODE_LONG_PKT_WRITE => {
            let ch1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };

            ch1.set_word_size(WordSize::BITS8);
            ch1.set_memory_address(DMA_BOX.buf.get() as usize as u32, true);
            ch1.set_peripheral_address(dr8b as usize as u32, false);
            ch1.set_transfer_length(256);

            ch1.set_direction(Direction::FromPeripheral);
            ch1.select_peripheral(hal::dmamux::DmaMuxIndex::SPI1_RX);
            ch1.enable();
            spi1.cr2.modify(|_r, w| w.rxdmaen().enabled());
            SPI_MODE.store(MODE_LONG_PKT_WRITE, Ordering::Relaxed);
        },
        _ => {
            // Nothing else to do, just wait for EXTI.
            SPI_MODE.store(MODE_INVALID_WAIT, Ordering::Relaxed);
        },
    }

}
