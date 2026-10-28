//! The [`Startup`] context: what the library knows about this boot, and
//! the operations a board's `main()` calls.

use crate::abi::startup::{STARTUP_HDR_SIGNATURE, StartupHeader};
use crate::cmdline::{CmdLine, Getopt, Opt};
use crate::global::Global;
use crate::kprintln;
use crate::ram::Ram;
use crate::syspage::Syspage;
use core::mem::MaybeUninit;

/// The options every startup understands, as in QNX's startup library.
/// A board adds its own letters (see [`Startup::options`]).
pub const COMMON_OPTIONS: &str = "ACc:D:F:f:I:i:K:M:N:o:P:R:S:Tvr:j:ZHQ:E:X:Uu:e:";

/// Startup's state during the boot.
pub struct Startup {
    /// The startup header that mkifs put in front of startup.
    pub shdr: StartupHeader,
    /// Physical address of the startup header.
    pub shdr_paddr: u64,
    pub cmdline: CmdLine<'static>,
    /// x0-x3 as the boot loader passed them to `_start`.
    pub boot_regs: [u64; 4],
    /// Verbosity: the number of `-v` options.
    pub debug_level: u32,
    /// Physical RAM, and what startup has taken of it.
    pub ram: Ram,
    /// The system page being built.
    pub syspage: Syspage,
}

/// The one Startup: too big for startup's stack, so it is a static.
static STARTUP: Global<MaybeUninit<Startup>> = Global::new(MaybeUninit::uninit());

impl Startup {
    /// Read the boot arguments and the startup header, once. Called by
    /// [`crate::entry!`] before the board's `main()`.
    #[doc(hidden)]
    pub fn init(boot_regs: [u64; 4]) -> &'static mut Self {
        let s = Self::new(boot_regs);
        // SAFETY: called once, by the boot CPU, before anything else uses
        // STARTUP; from now on the board's main() owns it.
        unsafe { (*STARTUP.get()).write(s) }
    }

    fn new(boot_regs: [u64; 4]) -> Self {
        let cmdline = match CmdLine::parse(crate::bootargs::area()) {
            Ok(c) => c,
            Err(e) => crate::crash!("bad boot arguments: {e:?}"),
        };
        let shdr_paddr = cmdline.shdr_addr;
        // SAFETY: mkifs put the startup header at this address, and the MMU
        // is off, so physical addresses are the ones to use.
        let shdr = unsafe { core::ptr::read_volatile(shdr_paddr as *const StartupHeader) };
        if shdr.signature != STARTUP_HDR_SIGNATURE {
            crate::crash!("no startup header at {shdr_paddr:#x}");
        }
        Self {
            shdr,
            shdr_paddr,
            cmdline,
            boot_regs,
            debug_level: 0,
            ram: Ram::new(),
            syspage: Syspage::new(),
        }
    }

    /// The command-line options: the library's [`COMMON_OPTIONS`] and
    /// the board's own letters, `board_options` (in `getopt()` syntax). A
    /// board handles its own options and passes the others to
    /// [`Startup::handle_common_option`].
    pub fn options(&self, board_options: &'static str) -> Getopt<'static> {
        Getopt::new(self.cmdline.argv, [COMMON_OPTIONS, board_options])
    }

    /// Apply one of the common options.
    pub fn handle_common_option(&mut self, opt: &Opt<'_>) {
        match *opt {
            Opt::Flag('v', _) => self.debug_level += 1,
            Opt::Flag(_, _) => {}
            Opt::Unknown(c) => kprintln!("startup: unknown option -{c}"),
        }
    }
}
