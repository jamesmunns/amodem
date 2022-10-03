use core::{cell::UnsafeCell, sync::atomic::{AtomicU8, Ordering}, mem::MaybeUninit};
use bbqueue_spicy::{BBBuffer, framed::{FrameGrantR, FrameGrantW}};
use stm32g0xx_hal::{dma::{C1, C2, C3, C4, DmaExt, Channel, WordSize, Direction}, rcc::Rcc, pac::{DMA, DMAMUX, SPI1}, dmamux::DmaMuxIndex};

use super::{gpios, spi::spi_int_unmask};

pub static PIPES: DataPipes = DataPipes {
    spi_to_rs485: Pipe::new(),
    rs485_to_spi: Pipe::new(),
    spi_rx: UnsafeCell::new(MaybeUninit::uninit()),
    spi_tx: UnsafeCell::new(MaybeUninit::uninit()),
    rs485_rx: UnsafeCell::new(MaybeUninit::uninit()),
    rs485_tx: UnsafeCell::new(MaybeUninit::uninit()),
};

pub struct Pipe {
    buffer: BBBuffer<1024>,

    wr_state: AtomicU8,
    wr_grant: UnsafeCell<MaybeUninit<FrameGrantW<'static, 1024>>>,

    rd_state: AtomicU8,
    rd_grant: UnsafeCell<MaybeUninit<FrameGrantR<'static, 1024>>>,
}

impl Pipe {
    const STATE_IDLE: u8 = 0;
    const STATE_GRANT_READY: u8 = 1;
    const STATE_GRANT_BUSY: u8 = 2;

    pub const fn new() -> Self {
        Self {
            buffer: BBBuffer::new(),
            wr_state: AtomicU8::new(Self::STATE_IDLE),
            wr_grant: UnsafeCell::new(MaybeUninit::uninit()),
            rd_state: AtomicU8::new(Self::STATE_IDLE),
            rd_grant: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn init(&'static self) {
        unsafe {
            self.buffer.init();
        }
        self.wr_state.store(Self::STATE_IDLE, Ordering::Relaxed);
        self.rd_state.store(Self::STATE_IDLE, Ordering::Relaxed);
    }

    pub fn service_lowprio_wr(&'static self) -> Option<(*mut u8, usize)> {
        match self.wr_state.load(Ordering::Acquire) {
            Self::STATE_IDLE => {
                let prod = unsafe { self.buffer.get_framed_producer() };
                match prod.grant(256) {
                    Ok(mut wgr) => unsafe {
                        let ptrlen: (*mut u8, usize) = (wgr.as_mut_ptr(), wgr.len());
                        let mu_ptr = self.wr_grant.get();
                        (&mut *mu_ptr).write(wgr);
                        self.wr_state.store(Self::STATE_GRANT_READY, Ordering::Release);
                        Some(ptrlen)
                    },
                    Err(_) => None,
                }
            }
            _ => None,
        }
    }

    pub fn service_lowprio_rd(&'static self) -> Option<(*const u8, usize)> {
        match self.rd_state.load(Ordering::Acquire) {
            Self::STATE_IDLE => {
                let cons = unsafe { self.buffer.get_framed_consumer() };
                match cons.read() {
                    Some(rgr) => unsafe {
                        let ptrlen: (*const u8, usize) = (rgr.as_ptr(), rgr.len());
                        let mu_ptr = self.rd_grant.get();
                        (&mut *mu_ptr).write(rgr);
                        self.rd_state.store(Self::STATE_GRANT_READY, Ordering::Release);
                        Some(ptrlen)
                    },
                    None => None,
                }
            }
            _ => None,
        }
    }

    #[inline]
    pub unsafe fn get_prep_wr_dma(&'static self) -> usize {
        if self.wr_state.load(Ordering::Relaxed) != Self::STATE_GRANT_READY {
            return 0;
        }
        self.wr_state.store(Self::STATE_GRANT_BUSY, Ordering::Relaxed);
        let mu_ptr = self.wr_grant.get();
        let slice: &mut [u8] = (*mu_ptr).assume_init_mut();
        slice.len()
    }

    #[inline]
    pub unsafe fn get_prep_rd_dma(&'static self) -> usize {
        if self.rd_state.load(Ordering::Relaxed) != Self::STATE_GRANT_READY {
            return 0;
        }
        self.rd_state.store(Self::STATE_GRANT_BUSY, Ordering::Relaxed);
        let mu_ptr = self.rd_grant.get();
        let slice: &mut [u8] = (*mu_ptr).assume_init_mut();
        slice.len()
    }

    #[inline]
    pub unsafe fn complete_wr_dma<F: FnOnce() -> usize>(&'static self, f: F) {
        if self.wr_state.load(Ordering::Relaxed) == Self::STATE_GRANT_BUSY {
            let used = f();
            let mu_ptr = self.wr_grant.get();
            let mut garbo = MaybeUninit::zeroed();
            core::ptr::swap(mu_ptr, &mut garbo);
            garbo.assume_init().commit(used);
            self.wr_state.store(Self::STATE_IDLE, Ordering::Relaxed);
        }
    }

    #[inline]
    pub unsafe fn complete_rd_dma(&'static self) {
        if self.rd_state.load(Ordering::Relaxed) == Self::STATE_GRANT_BUSY {
            let mu_ptr = self.rd_grant.get();
            let mut garbo = MaybeUninit::zeroed();
            core::ptr::swap(mu_ptr, &mut garbo);
            garbo.assume_init().release();
            self.rd_state.store(Self::STATE_IDLE, Ordering::Relaxed);
        }
    }
}

unsafe impl Sync for DataPipes { }

pub struct DataPipes {
    pub spi_to_rs485: Pipe,
    pub rs485_to_spi: Pipe,
    spi_rx: UnsafeCell<MaybeUninit<C1>>,
    spi_tx: UnsafeCell<MaybeUninit<C2>>,
    rs485_rx: UnsafeCell<MaybeUninit<C3>>,
    rs485_tx: UnsafeCell<MaybeUninit<C4>>,
}

impl DataPipes {
    pub unsafe fn init(&'static self, rcc: &mut Rcc, dma: DMA, dmamux: DMAMUX) {
        self.spi_to_rs485.init();
        self.rs485_to_spi.init();

        let dma = dma.split(rcc, dmamux);

        self.spi_rx.get().write(MaybeUninit::new(dma.ch1));
        self.spi_tx.get().write(MaybeUninit::new(dma.ch2));
        self.rs485_rx.get().write(MaybeUninit::new(dma.ch3));
        self.rs485_tx.get().write(MaybeUninit::new(dma.ch4));
    }

    #[inline]
    pub unsafe fn trigger_spi_rx_dma(&'static self) {
        let spi_rx: &mut C1 = (*self.spi_rx.get()).assume_init_mut();
        spi_rx.enable();
    }

    #[inline]
    pub unsafe fn trigger_spi_tx_dma(&'static self) {
        let spi_tx: &mut C2 = (*self.spi_tx.get()).assume_init_mut();
        spi_tx.enable();
    }

    #[inline]
    pub unsafe fn trigger_rs485_rx_dma(&'static self) {
        let rs485_rx: &mut C3 = (*self.rs485_rx.get()).assume_init_mut();
        rs485_rx.enable();
    }

    #[inline]
    pub unsafe fn trigger_rs485_tx_dma(&'static self) {
        let rs485_tx: &mut C4 = (*self.rs485_tx.get()).assume_init_mut();
        rs485_tx.enable();
    }

    #[inline]
    pub unsafe fn disable_spi_rx_dma(&'static self) {
        let spi_rx: &mut C1 = (*self.spi_rx.get()).assume_init_mut();
        spi_rx.disable();
    }

    #[inline]
    pub unsafe fn disable_spi_tx_dma(&'static self) {
        let spi_tx: &mut C2 = (*self.spi_tx.get()).assume_init_mut();
        spi_tx.disable();
    }

    #[inline]
    pub unsafe fn disable_rs485_rx_dma(&'static self) {
        let rs485_rx: &mut C3 = (*self.rs485_rx.get()).assume_init_mut();
        rs485_rx.disable();
    }

    #[inline]
    pub unsafe fn disable_rs485_tx_dma(&'static self) {
        let rs485_tx: &mut C4 = (*self.rs485_tx.get()).assume_init_mut();
        rs485_tx.disable();
    }

    pub fn idle_step(&'static self) {
        let spi1 = unsafe { &*SPI1::PTR };
        let dr8b: *mut u8 = spi1.dr.as_ptr().cast();

        let mut did_restore_spi = false;

        // rs485 read grant (outgoing)
        if let Some((ptr, len)) = self.spi_to_rs485.service_lowprio_rd() {
            // setup rs485 transmit dma, enable interrupt
            defmt::println!("Reloaded RS485 Read Grant (outgoing)");

            let rs485_tx: &mut C4 = unsafe { (*self.rs485_tx.get()).assume_init_mut() };

            unsafe { &*DMA::PTR }.ch4().cr.modify(|_, w| {
                w.psize().bits16();
                w.msize().bits8();
                w
            });

            rs485_tx.set_memory_address(ptr as usize as u32, true);
            rs485_tx.set_peripheral_address(dr8b as usize as u32, false);
            rs485_tx.set_transfer_length(len as u16);

            rs485_tx.set_direction(Direction::FromMemory);
            rs485_tx.select_peripheral(DmaMuxIndex::USART1_TX);
            did_restore_spi = true;
        }
        // spi write grant (incoming)
        if let Some((ptr, len)) = self.spi_to_rs485.service_lowprio_wr() {
            // setup spi receive dma, enable interrupt
            // mark "ready to receive spi" IO
            defmt::println!("Reloaded SPI Write Grant (incoming)");
            let spi_rx: &mut C1 = unsafe { (*self.spi_rx.get()).assume_init_mut() };

            spi_rx.set_word_size(WordSize::BITS8);
            spi_rx.set_memory_address(ptr as usize as u32, true);
            spi_rx.set_peripheral_address(dr8b as usize as u32, false);
            spi_rx.set_transfer_length(len as u16);

            spi_rx.set_direction(Direction::FromPeripheral);
            spi_rx.select_peripheral(DmaMuxIndex::SPI1_RX);

            gpios::set_rxrdy_active();
            did_restore_spi = true;
        }
        if did_restore_spi {
            defmt::println!("unmasked spi int!");
            spi_int_unmask();
        }

        // SPI read grant (outgoing)
        if let Some((ptr, len)) = self.rs485_to_spi.service_lowprio_rd() {
            // setup spi transmit dma, enable interrupt
            // mark "ready to send spi" IO
            defmt::println!("Reloaded SPI Read Grant (outgoing)");


            let spi_tx: &mut C2 = unsafe { (*self.spi_tx.get()).assume_init_mut() };

            spi_tx.set_word_size(WordSize::BITS8);
            spi_tx.set_memory_address(ptr as usize as u32, true);
            spi_tx.set_peripheral_address(dr8b as usize as u32, false);
            spi_tx.set_transfer_length(len as u16);

            spi_tx.set_direction(Direction::FromMemory);
            spi_tx.select_peripheral(DmaMuxIndex::SPI1_TX);

            gpios::set_txrdy_active();
        }

        // RS485 Write Grant (incoming)
        if let Some((ptr, len)) = self.rs485_to_spi.service_lowprio_wr() {
            // setup rs485 receive dma, enable interrupt
            defmt::println!("Reloaded RS485 Write Grant (incoming)");

            let rs485_rx: &mut C3 = unsafe { (*self.rs485_rx.get()).assume_init_mut() };

            unsafe { &*DMA::PTR }.ch3().cr.modify(|_, w| {
                w.psize().bits16();
                w.msize().bits8();
                w
            });

            rs485_rx.set_memory_address(ptr as usize as u32, true);
            rs485_rx.set_peripheral_address(dr8b as usize as u32, false);
            rs485_rx.set_transfer_length(len as u16);

            rs485_rx.set_direction(Direction::FromPeripheral);
            rs485_rx.select_peripheral(DmaMuxIndex::USART1_RX);
        }
    }
}
