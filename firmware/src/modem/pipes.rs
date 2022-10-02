use core::{cell::UnsafeCell, sync::atomic::{AtomicU8, Ordering}, mem::MaybeUninit};
use bbqueue_spicy::{BBBuffer, framed::{FrameGrantR, FrameGrantW}};

pub static PIPES: DataPipes = DataPipes {
    spi_to_rs485: Pipe::new(),
    rs485_to_spi: Pipe::new(),
};

struct Pipe {
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

    pub fn service_lowprio_wr(&'static self) -> bool {
        match self.wr_state.load(Ordering::Acquire) {
            Self::STATE_IDLE => {
                let prod = unsafe { self.buffer.get_framed_producer() };
                match prod.grant(256) {
                    Ok(wgr) => unsafe {
                        let mu_ptr = self.wr_grant.get();
                        (&mut *mu_ptr).write(wgr);
                        self.wr_state.store(Self::STATE_GRANT_READY, Ordering::Release);
                        true
                    },
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }

    pub fn service_lowprio_rd(&'static self) -> bool {
        match self.rd_state.load(Ordering::Acquire) {
            Self::STATE_IDLE => {
                let cons = unsafe { self.buffer.get_framed_consumer() };
                match cons.read() {
                    Some(rgr) => unsafe {
                        let mu_ptr = self.rd_grant.get();
                        (&mut *mu_ptr).write(rgr);
                        self.rd_state.store(Self::STATE_GRANT_READY, Ordering::Release);
                        true
                    },
                    None => false,
                }
            }
            _ => false,
        }
    }

    pub fn get_wr_dma(&'static self) -> Option<(*mut u8, usize)> {
        if self.wr_state.load(Ordering::Relaxed) != Self::STATE_GRANT_READY {
            return None;
        }

        let mu_ptr = self.wr_grant.get();
        let slice: &mut [u8] = unsafe { (*mu_ptr).assume_init_mut() };
        let len = slice.len();
        Some((slice.as_mut_ptr(), len))
    }

    pub fn get_rd_dma(&'static self) -> Option<(*const u8, usize)> {
        if self.rd_state.load(Ordering::Relaxed) != Self::STATE_GRANT_READY {
            return None;
        }

        let mu_ptr = self.rd_grant.get();
        let slice: &[u8] = unsafe { (*mu_ptr).assume_init_ref() };
        let len = slice.len();
        Some((slice.as_ptr(), len))
    }

    pub fn take_wr_dma(&'static self) -> Option<FrameGrantW<'static, 1024>> {
        if self.wr_state.load(Ordering::Relaxed) != Self::STATE_GRANT_BUSY {
            return None;
        }

        let mu_ptr = self.wr_grant.get();
        let mut garbo = MaybeUninit::zeroed();
        unsafe {
            core::ptr::swap(mu_ptr, &mut garbo);
            Some(garbo.assume_init())
        }
    }

    pub fn take_rd_dma(&'static self) -> Option<FrameGrantR<'static, 1024>> {
        if self.rd_state.load(Ordering::Relaxed) != Self::STATE_GRANT_BUSY {
            return None;
        }

        let mu_ptr = self.rd_grant.get();
        let mut garbo = MaybeUninit::zeroed();
        unsafe {
            core::ptr::swap(mu_ptr, &mut garbo);
            Some(garbo.assume_init())
        }
    }
}

unsafe impl Sync for DataPipes { }

pub struct DataPipes {
    pub spi_to_rs485: Pipe,
    pub rs485_to_spi: Pipe,
}

impl DataPipes {
    pub unsafe fn init(&'static self) {
        self.spi_to_rs485.init();
        self.rs485_to_spi.init();
    }

    fn idle_step(&'static self) {
        if self.spi_to_rs485.service_lowprio_rd() {
            // setup rs485 transmit dma, enable interrupt
        }
        if self.spi_to_rs485.service_lowprio_wr() {
            // setup spi receive dma, enable interrupt
            // mark "ready to receive spi" IO
        }
        if self.rs485_to_spi.service_lowprio_rd() {
            // setup spi transmit dma, enable interrupt
            // mark "ready to send spi" IO
        }
        if self.rs485_to_spi.service_lowprio_wr() {
            // setup rs485 receive dma, enable interrupt
        }
    }
}
