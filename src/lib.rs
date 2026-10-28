//! A QNX OS 8.0 startup library, in Rust.
//!
//! A QNX system boots in two steps. The *startup* program runs first: it
//! finds out what the hardware is, describes it in the *system page*,
//! prepares the MMU and the CPUs, and starts the kernel (`procnto`), which
//! never sees the hardware the way startup does. This crate is the generic
//! part of a startup program, the counterpart of QNX's `libstartup.a`. A
//! board crate adds what is specific to one board and calls [`entry!`].
//!
//! The crate has no board code. Board specifics come in through the
//! [`Board`] trait.

#![no_std]

pub mod arch;
pub mod bootargs;

/// What a board crate provides to the library.
pub trait Board {
    /// Write one byte to the board's debug console, waiting until the
    /// device can take it.
    fn debug_putc(&self, byte: u8);
}

/// Define the startup program's entry: `entry!(board_main)` makes the
/// library call `board_main()` once the CPU runs Rust code.
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[unsafe(no_mangle)]
        extern "C" fn __oscraft_startup_main() -> ! {
            $main()
        }
    };
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
