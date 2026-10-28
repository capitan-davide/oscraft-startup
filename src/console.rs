//! The debug console: startup's own output, `kprintln!`.
//!
//! The board picks the device (a UART) and its driver writes one byte at
//! a time. The same device later serves the kernel, through the debug
//! callouts that the board also provides.

use crate::global::Global;
use core::fmt;

// ANCHOR: debug_device
/// A device startup can print on.
pub trait DebugDevice: Sync {
    /// Write one byte, waiting until the device can take it.
    fn putc(&self, byte: u8);
}
// ANCHOR_END: debug_device

static CONSOLE: Global<Option<&'static dyn DebugDevice>> = Global::new(None);

/// Print on `device` from now on.
pub fn set(device: &'static dyn DebugDevice) {
    // SAFETY: only the boot CPU prints, so nothing else accesses CONSOLE.
    unsafe { *CONSOLE.get() = Some(device) };
}

struct Writer(&'static dyn DebugDevice);

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.0.putc(b'\r');
            }
            self.0.putc(b);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn print(args: fmt::Arguments) {
    // SAFETY: as in set().
    if let Some(device) = unsafe { *CONSOLE.get() } {
        let _ = fmt::write(&mut Writer(device), args);
    }
}

/// Print a line on the debug console, if there is one.
#[macro_export]
macro_rules! kprintln {
    () => { $crate::console::print(format_args!("\n")) };
    ($($arg:tt)*) => {
        $crate::console::print(format_args!("{}\n", format_args!($($arg)*)))
    };
}
