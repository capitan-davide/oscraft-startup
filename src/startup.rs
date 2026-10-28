//! The [`Startup`] context: what the library knows about this boot, and
//! the operations a board's `main()` calls.
//!
//! A board's `main()` follows the same order as QNX's startup programs:
//!
//! 1. set the debug console and parse the options;
//! 2. describe the RAM ([`Startup::add_ram`]) and the CPUs;
//! 3. build the page tables ([`Startup::init_mmu`]);
//! 4. set up the interrupt controller, the timer and the caches, and
//!    register the callouts (these map device registers for the kernel);
//! 5. add the board's strings and hwinfo;
//! 6. [`Startup::boot`]: load the kernel, finish the system page, start the
//!    other CPUs and jump.

use crate::abi::startup::{STARTUP_HDR_SIGNATURE, StartupHeader};
use crate::abi::syspage::{AS_NULL_OFF, CpupageEntry};
use crate::arch::aarch64::handoff::{self, MmuRegs};
use crate::arch::aarch64::mmu::{Mmu, PAGE_SIZE, Prot};
use crate::arch::aarch64::{cpu, gic_v2, smp};
use crate::cmdline::{CmdLine, Getopt, Opt, parse_size};
use crate::console::{self, DebugDriver};
use crate::global::Global;
use crate::kprintln;
use crate::ram::Ram;
use crate::syspage::{ASINFO_SPARE, MAX_CPUS, Syspage, strings};
use core::mem::{MaybeUninit, size_of};

/// The options every startup understands, as in QNX's startup library.
/// A board adds its own letters (see [`Startup::options`]).
pub const COMMON_OPTIONS: &str = "ACc:D:F:f:I:i:K:M:N:o:P:R:S:Tvr:j:ZHQ:E:X:Uu:e:";

// ANCHOR: startup_struct
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
    /// The most CPUs to start (`-P`).
    pub max_cpus: usize,
    /// Physical RAM, and what startup has taken of it.
    pub ram: Ram,
    /// The system page being built.
    pub syspage: Syspage,
    /// The page tables, once [`Startup::init_mmu`] has built them.
    pub mmu: Option<Mmu>,
    /// The CPUs' speed, for cpuinfo.
    pub cpu_speed_mhz: u32,
    /// How the board lets a secondary CPU run, and the `smp_spin`
    /// callout's reference (see [`Startup::init_smp`]).
    release_cpu: Option<fn(usize, u64)>,
    smp_spin: u64,
    /// The flattened device tree the boot loader passed (`-u`): its
    /// physical address and size.
    pub fdt: Option<(u64, u64)>,
    /// The board's debug UARTs, for `-D`.
    debug_drivers: &'static [DebugDriver],
}
// ANCHOR_END: startup_struct

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
            max_cpus: usize::MAX,
            ram: Ram::new(),
            syspage: Syspage::new(),
            mmu: None,
            cpu_speed_mhz: 0,
            release_cpu: None,
            smp_spin: 0,
            fdt: None,
            debug_drivers: &[],
        }
    }

    // ANCHOR: init_debug
    /// Use the first of `drivers` as the debug console, until `-D` selects
    /// another; [`Startup::boot`] gives the kernel the same device. Call it
    /// first, before the options, so that startup can print.
    pub fn init_debug(&mut self, drivers: &'static [DebugDriver]) {
        self.debug_drivers = drivers;
        if let Some((driver, port)) = console::select(drivers, None) {
            console::set(driver, port);
        }
    }
    // ANCHOR_END: init_debug

    /// The command-line options: the library's [`COMMON_OPTIONS`] and
    /// the board's own letters, `board_options` (in `getopt()` syntax). A
    /// board handles its own options and passes the others to
    /// [`Startup::handle_common_option`].
    pub fn options(&self, board_options: &'static str) -> Getopt<'static> {
        Getopt::new(self.cmdline.argv, [COMMON_OPTIONS, board_options])
    }

    // ANCHOR: common_options
    /// Apply one of the common options: `-v` (more output), `-D name.opts`
    /// (the debug UART), `-P n` (start at most n CPUs), `-N name` (the host
    /// name), `-r addr,size` (keep that RAM from the kernel), `-u reg|arg`
    /// (where the device tree's address is). The others are accepted and
    /// ignored.
    pub fn handle_common_option(&mut self, opt: &Opt<'_>) {
        match *opt {
            Opt::Flag('v', _) => self.debug_level += 1,
            Opt::Flag('P', Some(n)) => self.max_cpus = (parse_size(n).0 as usize).max(1),
            Opt::Flag('N', Some(name)) => self.syspage.add_typed_string(strings::CS_HOSTNAME, name),
            Opt::Flag('r', Some(arg)) => {
                let (addr, rest) = parse_size(arg);
                if let Some(size) = rest.strip_prefix(',') {
                    self.ram.reserve(addr, parse_size(size).0);
                }
            }
            Opt::Flag('D', Some(arg)) => match console::select(self.debug_drivers, Some(arg)) {
                Some((driver, port)) => console::set(driver, port),
                None => kprintln!("startup: no debug device {arg}"),
            },
            Opt::Flag('u', Some(arg)) => {
                if let Some(addr) = self.fdt_address(arg) {
                    self.set_fdt(addr);
                }
            }
            Opt::Flag(_, _) => {}
            Opt::Unknown(c) => kprintln!("startup: unknown option -{c}"),
        }
    }
    // ANCHOR_END: common_options

    // ANCHOR: fdt_address
    /// Where `-u` says the device tree is. `reg` (and, as in QNX's startup,
    /// anything but `arg`): its address is in x0 at entry. `arg`: the boot
    /// loader called startup as `main(argc, argv)`, as U-Boot's `go`
    /// command does, with the address in hexadecimal in `argv[1]`.
    fn fdt_address(&self, arg: &str) -> Option<u64> {
        if arg != "arg" {
            return Some(self.boot_regs[0]);
        }
        let (argc, argv) = (self.boot_regs[0], self.boot_regs[1]);
        if argc < 2 || argv == 0 {
            kprintln!("startup: -u arg, but no device tree address in argv");
            return None;
        }
        // SAFETY: the boot loader passed argv, an array of pointers to
        // NUL-terminated strings, in RAM; the MMU is off.
        let s = unsafe {
            let ptr = core::ptr::read_volatile((argv + 8) as *const u64);
            core::ffi::CStr::from_ptr(ptr as *const core::ffi::c_char)
        };
        let hex = s.to_str().ok()?.trim_start_matches("0x");
        u64::from_str_radix(hex, 16).ok()
    }
    // ANCHOR_END: fdt_address

    // ANCHOR: set_fdt
    /// Use the flattened device tree at `addr`, if there is one there: it
    /// stays out of startup's allocations and, for the drivers that read
    /// it, gets an asinfo entry, `fdt`.
    pub fn set_fdt(&mut self, addr: u64) {
        // The header: magic and total size, big-endian.
        // SAFETY: reading RAM with the MMU off; a wrong address gives a
        // wrong magic, which is checked.
        let [magic, size] = unsafe { core::ptr::read_volatile(addr as *const [u32; 2]) };
        if u32::from_be(magic) != 0xd00d_feed {
            kprintln!("startup: no device tree at {addr:#x}");
            return;
        }
        let size = u64::from(u32::from_be(size));
        self.ram.avoid(addr, size);
        self.fdt = Some((addr, size));
    }
    // ANCHOR_END: set_fdt

    /// Physical address of the image (the startup header).
    pub fn image_paddr(&self) -> u64 {
        u64::from(self.shdr.image_paddr) + self.shdr.addr_off
    }

    /// Physical address of the RAM the image runs in.
    pub fn ram_paddr(&self) -> u64 {
        u64::from(self.shdr.ram_paddr) + self.shdr.addr_off
    }

    // ANCHOR: add_ram
    /// Add `size` bytes of RAM at `start`, as an asinfo `ram` entry. The
    /// first call also makes the root of the asinfo tree, `memory` (the
    /// CPU's physical address range), and `below4G`; and takes the image's
    /// RAM (startup and the image file system) out of the free RAM.
    pub fn add_ram(&mut self, start: u64, size: u64) {
        if self.syspage.as_find("memory").is_none() {
            const BITS: [u32; 8] = [32, 36, 40, 42, 44, 48, 48, 48];
            let end = (1u64 << BITS[(cpu::mmfr0() & 7) as usize]) - 1;
            let mem = self.syspage.as_add(0, end, "memory", AS_NULL_OFF);
            self.syspage.as_add(0, (4 << 30) - 1, "below4G", mem);
        }
        self.syspage
            .as_add_containing(start, start + size - 1, "ram", "memory");
        self.ram.add(start, size);
        self.ram
            .reserve(self.ram_paddr(), u64::from(self.shdr.ram_size));
    }
    // ANCHOR_END: add_ram

    // ANCHOR: init_smp
    /// Use `num_cpus` CPUs (at most `-P`'s number). `release(cpu, entry)`
    /// is the board's way to let secondary CPU `cpu` run from the physical
    /// address `entry`. Must come before [`Startup::init_mmu`], which
    /// gives each CPU its page table.
    pub fn init_smp(&mut self, num_cpus: usize, release: fn(usize, u64)) {
        if self.mmu.is_some() {
            crate::crash!("init_smp() must come before init_mmu()");
        }
        let n = num_cpus.min(self.max_cpus).min(MAX_CPUS);
        self.syspage.num_cpu = n as u16;
        if n > 1 {
            self.release_cpu = Some(release);
            self.smp_spin = self.syspage.callout(&smp::SMP_SPIN, &[]);
        }
    }
    // ANCHOR_END: init_smp

    /// Build the page tables for the CPUs the system page describes, with
    /// startup itself mapped 1:1.
    pub fn init_mmu(&mut self) {
        let mut mmu = Mmu::new(
            &mut self.ram,
            usize::from(self.syspage.num_cpu),
            cpu::mmfr0(),
        );
        let start = self.image_paddr() & !(PAGE_SIZE - 1);
        let end =
            (self.image_paddr() + u64::from(self.shdr.startup_size)).next_multiple_of(PAGE_SIZE);
        mmu.map(&mut self.ram, Some(start), start, end - start, Prot::RWX);
        self.syspage.l0_vaddr = mmu.l0_vaddr;
        if self.debug_level > 0 {
            kprintln!(
                "MMU: TCR_EL1 {:#x}, L0 tables at {:#x}",
                mmu.tcr,
                mmu.l0_vaddr
            );
        }
        self.mmu = Some(mmu);
    }

    fn mmu(&mut self) -> (&mut Mmu, &mut Ram) {
        match self.mmu.as_mut() {
            Some(m) => (m, &mut self.ram),
            None => crate::crash!("init_mmu() must come first"),
        }
    }

    /// Map `size` bytes of device registers at `paddr` into the kernel's
    /// address space, for a callout, and return their virtual address.
    pub fn map_device(&mut self, paddr: u64, size: u64) -> u64 {
        let (mmu, ram) = self.mmu();
        mmu.map(ram, None, paddr, size, Prot::DEVICE)
    }

    /// Set up the generic timer as the system clock, with the tick on
    /// interrupt `intr` (27, the virtual timer's PPI, on most boards).
    pub fn init_qtime(&mut self, intr: i32) {
        cpu::init_timer();
        self.syspage.qtime = crate::qtime::qtime(cpu::cntfrq(), intr);
    }

    /// Describe the boot CPU, running at `speed_mhz`, and set up its GIC
    /// CPU interface.
    pub fn init_boot_cpu(&mut self, speed_mhz: u32) {
        self.cpu_speed_mhz = speed_mhz;
        cpu::init_cpuinfo(&mut self.syspage, 0, speed_mhz);
        gic_v2::init_cpu(&mut self.syspage.gic_map, 0, self.syspage.num_cpu);
    }

    // ANCHOR: boot
    /// Finish the boot: load the bootstrap executables, put the system
    /// page in its final place, and start the first executable.
    pub fn boot(&mut self) -> ! {
        let verbose = self.debug_level > 0;
        if self
            .syspage
            .typed_strings
            .find(strings::CS_HOSTNAME)
            .is_none()
        {
            self.syspage
                .add_typed_string(strings::CS_HOSTNAME, "localhost");
        }
        self.syspage.system_private.pagesize = PAGE_SIZE as u32;

        // The image's parts, for the kernel.
        let ifs = self.image_paddr() + u64::from(self.shdr.startup_size);
        let ifs_size = u64::from(crate::ifs::header(ifs).image_size);
        if self.shdr.flags1 & crate::abi::startup::STARTUP_HDR_FLAGS1_COMPRESS_MASK != 0 {
            crate::crash!("compressed image file systems are not supported");
        }
        let mem = self.syspage.as_find("memory").unwrap_or(AS_NULL_OFF);
        let (ram_paddr, startup_size) = (self.ram_paddr(), u64::from(self.shdr.startup_size));
        let ram_end = ram_paddr + u64::from(self.shdr.ram_size);
        self.syspage.as_add(ifs, ifs + ifs_size - 1, "imagefs", mem);
        self.syspage
            .as_add(ram_paddr, ram_paddr + startup_size - 1, "startup", mem);
        self.syspage
            .as_add(ram_paddr + startup_size, ram_end - 1, "bootram", mem);

        if let Some((fdt, size)) = self.fdt {
            self.syspage.as_add(fdt, fdt + size - 1, "fdt", mem);
            self.ram.reserve(fdt, size);
        }

        // The kernel's debug channel: the console's device, unless the
        // board set the callouts itself.
        if let Some((driver, port)) = console::current()
            && self.syspage.callout.debug[0].display_char == 0
        {
            let regs = self.map_device(port.base, driver.size);
            let sp = &mut self.syspage;
            let mut debug = |c: &'static crate::callout::Callout| {
                let values: &[u64] = if c.patches == 1 { &[regs] } else { &[] };
                sp.callouts.add(c, values)
            };
            let (d, p, b) = (
                debug(driver.display_char),
                debug(driver.poll_key),
                debug(driver.break_detect),
            );
            sp.callout.debug[0].display_char = d;
            sp.callout.debug[0].poll_key = p;
            sp.callout.debug[0].break_detect = b;
        }

        let (mmu, ram) = self.mmu();
        let entry = crate::ifs::load_bootstraps(mmu, ram, ifs, verbose);

        // The other CPUs describe themselves in the system page now.
        if let Some(release) = self.release_cpu
            && !smp::start_cpus(&mut self.syspage, self.cpu_speed_mhz, release)
        {
            crate::crash!("not all CPUs started (use -P to start fewer)");
        }

        // The system page's memory: the sections (with room for the
        // sysram entries still to come), the callouts, the cpupages.
        let n = usize::from(self.syspage.num_cpu);
        let sections = (self.syspage.layout().total_size + ASINFO_SPARE + SYSRAM_ROOM)
            .next_multiple_of(PAGE_SIZE as usize);
        let code = self
            .syspage
            .callouts
            .code_size()
            .next_multiple_of(PAGE_SIZE as usize);
        let size = (sections + code + n * size_of::<CpupageEntry>()) as u64;
        let (mmu, ram) = self.mmu();
        let Some(paddr) = ram.alloc(size, PAGE_SIZE) else {
            crate::crash!("no RAM for the system page");
        };
        let kern = mmu.map(ram, None, paddr, size, Prot::RWX);
        let user = mmu.map(
            ram,
            None,
            paddr,
            size,
            Prot {
                exec: true,
                user: true,
                ..Prot::RO
            },
        );
        // The callouts' code: read-only for the kernel too.
        let code_off = sections as u64;
        mmu.map(
            ram,
            Some(kern + code_off),
            paddr + code_off,
            code as u64,
            Prot::RX,
        );
        let (tcr, ttbr0) = (mmu.tcr, mmu.ttbr0);
        let ttbr1: [u64; MAX_CPUS] = core::array::from_fn(|c| mmu.ttbr1(c));

        // What is still free is the kernel's. Nothing is allocated after
        // this.
        for (start, end) in self.ram.free() {
            if self.debug_level > 2 {
                kprintln!("sysram {start:#x}-{:#x}", end - 1);
            }
            self.syspage
                .as_add_containing(start, end - 1, "sysram", "ram");
        }

        let sp = &mut self.syspage;
        let cpupage_off = code_off + code as u64;
        sp.system_private.kern_syspageptr = kern;
        sp.system_private.user_syspageptr = user;
        sp.system_private.kern_cpupageptr = kern + cpupage_off;
        sp.system_private.user_cpupageptr = user + cpupage_off;
        sp.system_private.cpupage_spacing = size_of::<CpupageEntry>() as u32;

        // SAFETY: RAM startup just allocated for this; the MMU is off.
        let memory = unsafe { core::slice::from_raw_parts_mut(paddr as *mut u8, size as usize) };
        let addrs = sp
            .callouts
            .output(&mut memory[sections..sections + code], kern + code_off);
        sp.resolve_callouts(&addrs);
        let layout = sp.layout();
        if layout.total_size + ASINFO_SPARE > sections {
            crate::crash!(
                "system page: {} bytes do not fit in {sections}",
                layout.total_size
            );
        }
        sp.write(&layout, &mut memory[..sections]);
        for cpu in 0..n {
            let page = CpupageEntry {
                cpu: cpu as u32,
                ..Default::default()
            };
            let at = cpupage_off as usize + cpu * size_of::<CpupageEntry>();
            // SAFETY: inside the system page's memory.
            unsafe { core::ptr::write_unaligned(memory[at..].as_mut_ptr().cast(), page) };
        }
        if verbose {
            kprintln!("System page at phys:{paddr:#x} user:{user:#x} kern:{kern:#x}");
            kprintln!("Starting next program at {entry:#x}");
        }
        if n > 1 {
            let pending = paddr + u64::from(layout.header.smp.entry_off);
            let smp_spin = sp.callouts.resolve(self.smp_spin, &addrs);
            let transfer = smp::Transfer {
                syspage: kern,
                smp_spin,
                tcr,
                ttbr0,
                ttbr1,
                pending,
            };
            smp::transfer_cpus(transfer, n);
        }
        let regs = MmuRegs::new(tcr, ttbr0, ttbr1[0]);
        // SAFETY: the page tables map startup 1:1, and the kernel and the
        // system page in the kernel's half.
        unsafe { handoff::vstart(kern, entry, 0, &regs) }
    }
    // ANCHOR_END: boot
}

/// Room in the system page for the sysram entries, added after the
/// system page's memory is allocated.
const SYSRAM_ROOM: usize = 16 * size_of::<crate::abi::syspage::AsinfoEntry>();
