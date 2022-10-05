use core::sync::atomic::{AtomicU8, Ordering};

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

const MODE_IDLE: u8 = 0;
const MODE_TODO: u8 = 1;

pub fn rs485_isr() {
    let mode = MODE.load(Ordering::Relaxed);
    match mode {
        MODE_IDLE => idle_start(),
        _ => defmt::panic!(),
    }
}

fn idle_start() {
    // Blocking wait for three words.
    //
    // Since we were interrupted AFTER the first word arrived, it should take
    // 22 bit periods, or 2.75uS to complete this processing. Since that is only
    // 176 cycles, don't waste time waiting for another interrupt. SPI can still
    // interrupt us.
    //
    // Set a timeout for 5uS to prevent deadlock.
    let usart1 = unsafe { &*USART1::PTR };
    let mut rxbuf = [0u16; 3];
    let timer = GlobalRollingTimer::new();
    let start = timer.get_ticks();

    let tx_amt = unsafe { pipes::PIPES.spi_to_rs485.get_prep_rd_dma() };
    let rx_amt = unsafe { pipes::PIPES.rs485_to_spi.get_prep_wr_dma() };

    let res = rxbuf.iter_mut().try_for_each(|b| {
        loop {
            if usart1.isr.read().rxne().bit_is_set() {
                *b = usart1.rdr.read().rdr().bits();
                return Ok(());
            }
            if timer.micros_since(start) >= 5 {
                return Err(());
            }
        }
    });
    defmt::assert!(res.is_ok(), "TIMEOUT");
    defmt::assert_ne!(tx_amt, 0, "NONE?");

    let len = [rxbuf[1] as u8, rxbuf[2] as u8];
    // TODO mix _len with tx_amt to determine actual amount to send
    let _len = u16::from_le_bytes(len);

    let txb = (tx_amt as u16).to_le_bytes();
    let rxb = (rx_amt as u16).to_le_bytes();

    txb.iter().chain(rxb.iter()).for_each(|b| {
        usart1.tdr.write(|w| w.tdr().bits((*b) as u16));
    });

    // usart1.cr3.modify(|_r, w| w.dmat().enabled());

    // MODE.store(MODE_TODO, Ordering::Relaxed);
    // unsafe {
    //     pipes::PIPES.trigger_rs485_tx_dma();
    // }
    usart1.icr.write(|w| w.cmcf().set_bit());

    // defmt::println!("started dma...");
}
