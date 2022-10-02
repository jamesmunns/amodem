use core::{sync::atomic::{AtomicU8, Ordering, AtomicU16}, cell::UnsafeCell, mem::MaybeUninit};

use stm32g0xx_hal::{rcc::{Rcc, Enable, Reset}, pac::SPI1, dma::{C1, C2, Channel, WordSize, Direction}, dmamux::DmaMuxIndex};

use super::pipes;

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

unsafe impl Sync for DmaBox { }

struct DmaBox {
    ch1: UnsafeCell<MaybeUninit<C1>>,
    ch2: UnsafeCell<MaybeUninit<C2>>,
}

static DMA_BOX: DmaBox = DmaBox {
    ch1: UnsafeCell::new(MaybeUninit::uninit()),
    ch2: UnsafeCell::new(MaybeUninit::uninit()),
};


#[inline]
pub fn setup_spi(
    rcc: &mut Rcc,
    spi1: SPI1,
    tx_dma: C1,
    rx_dma: C2,
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

#[inline]
pub fn spi_dr_u8() -> *mut u8 {
    let spi1 = unsafe { &*SPI1::PTR };
    let dr8b: *mut u8 = spi1.dr.as_ptr().cast();
    dr8b
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
        MODE_LONG_PKT_READWRITE => {
            // Write len
            let tx_gr = pipes::PIPES.rs485_to_spi.get_rd_dma();
            let tx_amt = match tx_gr {
                Some((_, len)) => len,
                None => 0
            };
            unsafe {
                dr16b.write_volatile(tx_amt as u16);
            };

            let ch1: &mut C1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };
            let ch2: &mut C2 = unsafe { (*DMA_BOX.ch2.get()).assume_init_mut() };

            ch1.set_word_size(WordSize::BITS8);
            ch1.set_memory_address(DMA_BOX.buf.get() as usize as u32, true);
            ch1.set_peripheral_address(dr8b as usize as u32, false);
            ch1.set_transfer_length(256);

            ch1.set_direction(Direction::FromMemory);
            ch1.select_peripheral(DmaMuxIndex::SPI1_TX);
            ch1.enable();
            spi1.cr2.modify(|_r, w| w.txdmaen().enabled());
            SPI_MODE.store(MODE_LONG_PKT_READ, Ordering::Relaxed);
        },
        // MODE_LONG_PKT_WRITE => {
        //     let ch1: C1 = unsafe { (*DMA_BOX.ch1.get()).assume_init_mut() };

        //     ch1.set_word_size(WordSize::BITS8);
        //     ch1.set_memory_address(DMA_BOX.buf.get() as usize as u32, true);
        //     ch1.set_peripheral_address(dr8b as usize as u32, false);
        //     ch1.set_transfer_length(256);

        //     ch1.set_direction(Direction::FromPeripheral);
        //     ch1.select_peripheral(DmaMuxIndex::SPI1_RX);
        //     ch1.enable();
        //     spi1.cr2.modify(|_r, w| w.rxdmaen().enabled());
        //     SPI_MODE.store(MODE_LONG_PKT_WRITE, Ordering::Relaxed);
        // },
        _ => {
            // Nothing else to do, just wait for EXTI.
            SPI_MODE.store(MODE_INVALID_WAIT, Ordering::Relaxed);
        },
    }
}
