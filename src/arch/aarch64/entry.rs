//! The first instructions: `_start`.
//!
//! The boot loader enters the image at `_start` on the boot CPU, with the
//! MMU and caches off and x0-x3 holding its arguments (on the Raspberry Pi,
//! x0 is the device tree's address). `_start` saves x0-x3, sets up a stack
//! and calls Rust.

use core::arch::global_asm;

/// Size of the boot CPU's stack.
const STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct Stack([u8; STACK_SIZE]);

#[unsafe(no_mangle)]
static mut STARTUP_STACK: Stack = Stack([0; STACK_SIZE]);

/// x0-x3 as the boot loader left them.
#[unsafe(no_mangle)]
pub static mut BOOT_REGS: [u64; 4] = [0; 4];

// ANCHOR: start
global_asm!(
    ".section .text._start, \"ax\"",
    ".global _start",
    "_start:",
    // No interrupts until the kernel takes over.
    "    msr     daifset, #0xf",
    // Save the boot loader's arguments.
    "    adrp    x9, BOOT_REGS",
    "    add     x9, x9, :lo12:BOOT_REGS",
    "    stp     x0, x1, [x9]",
    "    stp     x2, x3, [x9, #16]",
    // A stack, at the top of STARTUP_STACK.
    "    adrp    x9, STARTUP_STACK",
    "    add     x9, x9, :lo12:STARTUP_STACK",
    "    add     x9, x9, #{stack_size}",
    "    mov     sp, x9",
    "    bl      __oscraft_startup_main",
    "0:  wfe",
    "    b       0b",
    stack_size = const STACK_SIZE,
);
// ANCHOR_END: start
