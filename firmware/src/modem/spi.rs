use core::sync::atomic::{AtomicU8, Ordering, AtomicU16};

use stm32g0xx_hal::{rcc::{Rcc, Enable, Reset}, pac::{SPI1, EXTI, DMA}};

use super::{pipes, gpios};

static SPI_MODE: AtomicU8 = AtomicU8::new(MODE_IDLE);
const MODE_IDLE: u8 = 0b000_00000;
const MODE_RELOAD: u8 = 0b000_00001;

// TODO: This should probably be an enum or something. Be careful when updating.
//
const MODE_MASK: u8 = 0b111_00000;

const MODE_LONG_PKT_READWRITE: u8 = 0b001_00000;
const MODE_SHORT_REG_READ: u8 = 0b011_00000;
const MODE_SHORT_REG_WRITE: u8 = 0b100_00000;
const MODE_INVALID_WAIT: u8 = 0b111_00000;
//
// ENDTODO

const ONE_ATOMIC: AtomicU16 = AtomicU16::new(0xACAB);
static REGS: [AtomicU16; 32] = [ONE_ATOMIC; 32];


#[inline]
pub fn setup_spi(
    rcc: &mut Rcc,
    spi1: SPI1,
) {
    SPI1::enable(rcc);
    SPI1::reset(rcc);

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

    spi1.cr1.modify(|_r, w| w.spe().enabled());
}

pub fn spi_int_unmask() {
    let spi1 = unsafe { &*SPI1::PTR };
    spi1.cr2.modify(|_r, w| w.rxneie().not_masked());
}

#[inline]
pub fn spi_dr_u8() -> *mut u8 {
    let spi1 = unsafe { &*SPI1::PTR };
    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    dr8b
}

#[inline]
pub fn exti_isr() {
    let val = SPI_MODE.load(Ordering::Relaxed);
    let mode = val & MODE_MASK;
    let low = val & !MODE_MASK;

    let exti = unsafe { &*EXTI::PTR };
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
        MODE_LONG_PKT_READWRITE => {
            spi1.cr2.modify(|_r, w| {
                w.txdmaen().disabled();
                w.rxdmaen().disabled();
                w
            });
            unsafe {
                pipes::PIPES.spi_to_rs485.complete_wr_dma(|| {
                    let dma = &*DMA::PTR;
                    let remain = dma.ch1().ndtr.read().ndt().bits();
                    remain as usize
                });
                pipes::PIPES.rs485_to_spi.complete_rd_dma();
            }
            unsafe {
                pipes::PIPES.disable_spi_rx_dma();
                pipes::PIPES.disable_spi_tx_dma();
            }

        },
        _ => {
            // Huh, that was weird.
        },
    }

    while !spi1.sr.read().rxne().is_empty() {
        let _ = unsafe { dr8b.read_volatile() };
    }

    // TODO: Drain TX FIFO?
    SPI_MODE.store(MODE_IDLE, Ordering::Relaxed);
}

#[inline]
pub fn spi_isr() {

    let spi1 = unsafe { &*SPI1::PTR };
    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    let dr16b: *mut u16 = spi1.dr.as_ptr().cast();

    // Disable RXNE interrupt
    spi1.cr2.modify(|_r, w| w.rxneie().masked());

    // defmt::assert!(!spi1.sr.read().rxne().is_empty());
    let mode = SPI_MODE.load(Ordering::Relaxed);
    if mode != MODE_IDLE {
        defmt::panic!("Not idle?");
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
        MODE_LONG_PKT_READWRITE => {
            // Write len
            let tx_amt = unsafe { pipes::PIPES.rs485_to_spi.get_prep_rd_dma() };
            unsafe {
                dr16b.write_volatile(tx_amt as u16);
            };
            // This is the measuring point for "did we get a response back in time"
            // v
            // X
            // ^
            let rx_amt = unsafe { pipes::PIPES.spi_to_rs485.get_prep_wr_dma() }; // START

            if tx_amt != 0 {
                spi1.cr2.modify(|_r, w| w.txdmaen().enabled());
                unsafe { pipes::PIPES.trigger_spi_tx_dma() };
                gpios::set_txrdy_inactive();
            }
            if rx_amt != 0 {
                spi1.cr2.modify(|_r, w| w.rxdmaen().enabled());
                unsafe { pipes::PIPES.trigger_spi_rx_dma() };
                gpios::set_rxrdy_inactive();                                     // END - 108 cycles: TODO look at this
            }

            SPI_MODE.store(MODE_LONG_PKT_READWRITE, Ordering::Relaxed);
        },
        _ => {
            // Nothing else to do, just wait for EXTI.
            SPI_MODE.store(MODE_INVALID_WAIT, Ordering::Relaxed);
        },
    }
}
