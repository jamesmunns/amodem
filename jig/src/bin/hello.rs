#![no_main]
#![no_std]

use jig as _; // global logger + panicking-behavior + memory layout

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::println!("Hello, world!");

    jig::exit()
}
