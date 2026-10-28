# oscraft-startup

A QNX OS 8.0 startup library in Rust: the generic part of a QNX startup
program, the counterpart of QNX's `libstartup.a`. A board crate adds the
board's facts and code, and boots QNX's own kernel.

It builds the system page, copies and patches the kernel callouts, sets
up the AArch64 page tables and the GICv2, loads the bootstrap
executables, starts the secondary CPUs and hands over to the kernel. It
has no board code.

- `src/`: the library.
- `abi/`: the layouts of QNX's C structures, recorded from the SDP's
  headers, which the unit tests check the Rust types against.
- `startup.ld`: the partial-link script for the startup program mkifs
  takes.
- `book/`: the library's book.

## Tests and the book

```text
cargo test                  # unit tests, on the host
python3 -I book/check.py    # the book's references to the code
mdbook build book           # the book
```

`book/book.toml` builds the book next to the BSP book of the repository
it lives in; in a checkout of the library alone, pass
`--dest-dir book/build` to `mdbook build`.
