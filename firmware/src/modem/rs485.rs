use core::sync::atomic::{AtomicU8, Ordering, AtomicU16};

use groundhog::RollingTimer;
use stm32g0xx_hal::{rcc::{Rcc, Enable, Reset}, pac::USART1};

use crate::{GlobalRollingTimer, modem::pipes};

pub fn setup_rs485(rcc: &mut Rcc, usart1: USART1) {
    USART1::enable(rcc);
    USART1::reset(rcc);

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
        w.te().disabled();
        w.re().disabled();
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
        w.rxftcfg().depth_1_4();
        w.tcbgtie().disabled();
        w.txftie().disabled();
        w.wufie().disabled();
        // w.wus();
        // w.scarcnt();
        w.dep().high();
        w.dem().enabled();
        w.ddre().disabled();
        w.ovrdis().enabled();
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

    // usart1.rtor.modify(|_r, w| {
    //     w
    // });

    usart1.cr1.modify(|_r, w| w.ue().enabled());

    let timer = GlobalRollingTimer::new();


    usart1.cr1.modify(|_r, w| {
        w.te().enabled();
        w.re().enabled();
        w
    });

    let start = timer.get_ticks();
    // Request to enter mute mode
    usart1.rqr.write(|w| w.mmrq().set_bit());

    // Wait until the "is in mute mode" bit is set
    while usart1.isr.read().rwu().bit_is_clear() { }
    defmt::println!("Took {}us", timer.micros_since(start));
    // Empty the FIFO
    usart1.rqr.write(|w| w.rxfrq().set_bit());

    defmt::println!("ISR: {:08X}", usart1.isr.read().bits());

    MODE.store(MODE_IDLE, Ordering::Relaxed);
    RECV_AMT.store(0, Ordering::Relaxed);
}

pub fn enable_rs485_addr_match() {
    let usart1 = unsafe { &*USART1::PTR };
    let timer = GlobalRollingTimer::new();
    let start = timer.get_ticks();

    usart1.cr1.modify(|_r, w| {
        w.cmie().enabled();
        w
    });
}

static MODE: AtomicU8 = AtomicU8::new(MODE_IDLE);
static RECV_AMT: AtomicU16 = AtomicU16::new(0);

const MODE_IDLE: u8 = 0;
const MODE_SEND_NO_DMA: u8 = 1;
const MODE_SEND_DMA: u8 = 2;
const MODE_RECV: u8 = 3;

pub fn rs485_isr() {
    let mode = MODE.load(Ordering::Relaxed);
    match mode {
        // Idle:
        // On: Addr match + data (RXNE)
        // do: Wait for (addr_u16, Rrx_cap, Rtx_cap), send (Mtx_len, Mrx_cap),
        //       start data tx (if any), enable TCIE
        // then: wait for tx complete
        MODE_IDLE => idle_start(),
        MODE_SEND_DMA => dma_tx_complete(),
        MODE_SEND_NO_DMA => no_dma_tx_complete(),
        MODE_RECV => recv_complete(),
        // TX Done:
        // On:
        _ => defmt::panic!(),
    }
}

fn recv_complete() {
    defmt::panic!("recv_complete");
}

fn no_dma_tx_complete() {
    defmt::panic!("no_dma_tx_complete");
}

fn dma_tx_complete() {
    defmt::panic!("dma_tx_complete");
}

// TODO: start some kind of timer for 1ms (?), if it hits,
// abort current cycle, log error. If we hit the end of cycle
// before that, defuse.

fn idle_start() {
    // Blocking wait for five words.
    //
    // Since we were interrupted AFTER the first word arrived, it should take
    // 44 bit periods, or 5.50uS to complete this processing. Since that is only
    // (2 * 176) cycles, don't waste time waiting for another interrupt. SPI can still
    // interrupt us.
    //
    // Set a timeout for 8uS to prevent deadlock.
    let usart1 = unsafe { &*USART1::PTR };
    let mut rxbuf = [0u16; 5];
    let timer = GlobalRollingTimer::new();
    let start = timer.get_ticks();

    // Clear character match flag
    usart1.icr.write(|w| w.cmcf().set_bit());

    let tx_amt_cap = unsafe { pipes::PIPES.spi_to_rs485.get_prep_rd_dma() };
    let rx_amt_cap = unsafe { pipes::PIPES.rs485_to_spi.get_prep_wr_dma() };

    let res = rxbuf.iter_mut().try_for_each(|b| {
        loop {
            if usart1.isr.read().rxne().bit_is_set() {
                *b = usart1.rdr.read().rdr().bits();
                return Ok(());
            }
            if timer.micros_since(start) >= 8 {
                return Err(());
            }
        }
    });

    // TODO: replace this with logging the timeout and
    // a req to return to mute mode
    defmt::assert!(res.is_ok(), "TIMEOUT");

    let len_rx = [rxbuf[1] as u8, rxbuf[2] as u8];
    let len_tx = [rxbuf[3] as u8, rxbuf[4] as u8];
    // TODO mix _len with tx_amt to determine actual amount to send
    let r_rx_cap = u16::from_le_bytes(len_rx);
    let r_tx_cap = u16::from_le_bytes(len_tx);

    let tx_amt = if (r_rx_cap as usize) >= tx_amt_cap {
        // Router can hold what we're sending (including if we want to
        // send zero bytes)
        tx_amt_cap
    } else {
        // Router CAN'T hold what we're sending, send nothing
        unsafe {
            pipes::PIPES.spi_to_rs485.abort_rd_dma();
        }
        0
    };
    let rx_amt = if (r_tx_cap as usize) <= rx_amt_cap {
        // We can hold what the router is sending (including if it wants
        // to send zero bytes)
        rx_amt_cap
    } else {
        unsafe {
            pipes::PIPES.rs485_to_spi.abort_wr_dma();
        }
        0
    };

    let txb: [u8; 2] = (tx_amt as u16).to_le_bytes();
    let rxb: [u8; 2] = (rx_amt as u16).to_le_bytes();

    txb.iter().chain(rxb.iter()).for_each(|b| {
        usart1.tdr.write(|w| w.tdr().bits((*b) as u16));
    });

    // Enable TX complete interrupt. ISR.TC is cleared on write to TDR.
    usart1.cr1.modify(|_r, w| {
        w.tcie().enabled();
        w
    });

    // Store amount to receive
    RECV_AMT.store(rx_amt as u16, Ordering::Relaxed);

    if tx_amt != 0 {
        usart1.cr3.modify(|_r, w| w.dmat().enabled());

        MODE.store(MODE_SEND_DMA, Ordering::Relaxed);
        unsafe {
            pipes::PIPES.trigger_rs485_tx_dma();
        }
    } else {
        MODE.store(MODE_SEND_NO_DMA, Ordering::Relaxed);
    }

    // defmt::println!("started dma...");
}
