# The Startup Context

Once `_start` has a stack, control passes to Rust, and from the library
to the board. This chapter shows how the two meet: the `entry!` macro,
the `Startup` context the board works with, the command line, and the
debug console.

## From `_start` to the board's `main()`

A board crate names its `main()` with `entry!` (Listing 4.1). The macro
defines the symbol `_start` calls, `__oscraft_startup_main`, which builds
the context and passes it on.

*Listing 4.1: The `entry!` macro (`src/lib.rs`).*

```rust
{{#include ../../src/lib.rs:entry_macro}}
```

The board's `main()` never returns: it ends by calling `Startup::boot()`,
which jumps into the kernel (Chapter 13).

## The `Startup` context

`Startup` (Listing 4.2) holds everything the library knows about the boot,
and its methods are the operations a board calls, in the order of
Table 1.1. It plays the part of QNX's startup library's global variables
(`shdr`, `debug_flag`, `lsp`, ...).

*Listing 4.2: The context, `Startup` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:startup_struct}}
```

With the system page builder inside it, `Startup` takes about 13 KB, most
of startup's 16 KB stack. It is a static instead, initialized once by
`Startup::init()` and handed to `main()` as a `&'static mut`.

> [!NOTE]
> Startup has very little global state, and no locks. Until the secondary
> CPUs start, one CPU runs; when they do, the boot CPU waits for each in
> turn (Chapter 12). The few globals use a `Global<T>` wrapper
> (`src/global.rs`) whose every access is `unsafe` and says why it is safe.

## The command line

`Startup::init()` first parses the boot-arguments area that mkifs filled
in (Chapter 2). Listing 4.3 splits it into the argument and environment
strings, and finds the startup header's address in it.

*Listing 4.3: Parsing the boot arguments, `CmdLine::parse()` (`src/cmdline.rs`).*

```rust
{{#include ../../src/cmdline.rs:cmdline}}
```

The strings stay in the area; `CmdLine` only holds references to them, up
to 32 of each kind, which is plenty for a startup command line.

QNX's startup programs read their options with `getopt()`. The library
has the same parser (Listing 4.4): single-letter options, grouped
(`-vvv`), with an argument either attached (`-Dminiuart`) or in the next
word (`-D miniuart`).

*Listing 4.4: The option parser, `Getopt` (`src/cmdline.rs`).*

```rust
{{#include ../../src/cmdline.rs:getopt}}
```

A board parses its own options and the library's in one loop. The library's
letters are `COMMON_OPTIONS`, the same set as QNX's startup library, so an
existing build file's options are accepted; the board adds its own, such
as `-W` for a watchdog:

```rust
for opt in s.options("W:") {
    match opt {
        Opt::Flag('W', ms) => { /* the board's option */ }
        other => s.handle_common_option(&other),
    }
}
```

Listing 4.5 shows the common options the library acts on. Appendix A
lists all of them.

*Listing 4.5: The common options, `Startup::handle_common_option()` (`src/startup.rs`).*

```rust
{{#include ../../src/startup.rs:common_options}}
```

## The debug console

Startup prints on a UART the board chooses: its messages with `-v`, and
its crash reports. The board provides the device as a `DebugDevice`
(Listing 4.6), a single method that writes one byte, and selects it with
`console::set()`.

*Listing 4.6: The console device, `DebugDevice` (`src/console.rs`).*

```rust
{{#include ../../src/console.rs:debug_device}}
```

On top of it, `kprintln!` formats with Rust's `core::fmt`, and `crash!`
prints a message and stops the CPU. A panic does the same, so a failed
`assert!` in startup ends with a message on the console rather than a
silent hang.

The kernel needs a debug channel too, but it cannot call startup's
`DebugDevice`: startup is gone by then. The board gives the kernel its own
copy of the code, as callouts (Chapter 7).
