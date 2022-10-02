use stm32g0xx_hal::{rcc::{Rcc, Config, RccExt, PllConfig, Prescaler}, pac::{RCC, TIM2}};

use crate::GlobalRollingTimer;

pub mod spi;
pub mod gpios;
pub mod rs485;
pub mod pipes;

#[inline]
pub fn setup_sys_clocks(rcc: RCC) -> Rcc {
    let config = Config::pll()
        .pll_cfg(PllConfig::with_hsi(1, 8, 2))
        .ahb_psc(Prescaler::NotDivided)
        .apb_psc(Prescaler::NotDivided);
    rcc.freeze(config)
}

#[inline]
pub fn setup_rolling_timer(timer: TIM2) {
    GlobalRollingTimer::init(timer);
}
