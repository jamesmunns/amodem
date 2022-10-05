use groundhog::RollingTimer;
use stm32g0xx_hal::{rcc::{Rcc, Enable, Reset}, pac::USART1};

use crate::GlobalRollingTimer;

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
        w.deat().variant(31); // assertion: one bit time. TODO: check xcvr timing
        w.dedt().variant(31); // deassertn: one bit time. TODO: check xcvr timing
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
        w.rxftcfg();
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
    usart1.rqr.write(|w| w.mmrq().set_bit());
    while usart1.isr.read().rwu().bit_is_clear() { }
    defmt::println!("Took {}us", timer.micros_since(start));
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
