//! Decompressing a compressed image file system.
//!
//! mkifs can compress the image file system (not startup, which must run
//! first): with `+compress`, startup's header says which algorithm, and
//! the image file system is a series of blocks, each a big-endian 16-bit
//! length and that many bytes of compressed data, decompressing to at
//! most 64 KB, until a length of 0. Two algorithms make bootable images:
//! LZO1X and UCL's NRV2B (mkifs's default, "UCL8"). Each block is
//! compressed on its own, so the decompressors need no state between
//! blocks.
//!
//! Both algorithms are LZ77 variants: the stream mixes literal bytes with
//! *matches*, copies of earlier output given as a distance back and a
//! length. They differ in how they encode the two.

/// Why a stream could not be decompressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input ended in the middle of a block.
    InputOverrun,
    /// The output would not fit.
    OutputOverrun,
    /// A match points before the start of the output.
    LookBehind,
}

/// The compression algorithms startup supports (`STARTUP_HDR_FLAGS1_COMPRESS_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Lzo,
    Ucl,
}

impl Algorithm {
    /// The algorithm of a startup header's `flags1`, if supported.
    pub fn from_flags1(flags1: u8) -> Option<Self> {
        match flags1 & crate::abi::startup::STARTUP_HDR_FLAGS1_COMPRESS_MASK {
            0x08 => Some(Algorithm::Lzo),
            0x0c => Some(Algorithm::Ucl),
            _ => None,
        }
    }
}

// ANCHOR: decompress_blocks
/// Decompress the blocks at the start of `src` into `dst`. Returns the
/// number of bytes written.
pub fn decompress(algorithm: Algorithm, src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let (mut ip, mut op) = (0, 0);
    loop {
        let len = match src.get(ip..ip + 2) {
            Some(b) => usize::from(b[0]) << 8 | usize::from(b[1]),
            None => return Err(Error::InputOverrun),
        };
        ip += 2;
        if len == 0 {
            return Ok(op);
        }
        let block = src.get(ip..ip + len).ok_or(Error::InputOverrun)?;
        op += match algorithm {
            Algorithm::Lzo => lzo1x(block, &mut dst[op..])?,
            Algorithm::Ucl => nrv2b(block, &mut dst[op..])?,
        };
        ip += len;
    }
}
// ANCHOR_END: decompress_blocks

/// Output with bounds checks, and copies of earlier output.
struct Out<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl Out<'_> {
    fn push(&mut self, b: u8) -> Result<(), Error> {
        *self.buf.get_mut(self.pos).ok_or(Error::OutputOverrun)? = b;
        self.pos += 1;
        Ok(())
    }

    /// Copy `len` bytes from `dist` bytes back, a byte at a time: the copy
    /// may overlap what it writes (a run of the same bytes).
    fn copy_match(&mut self, dist: usize, len: usize) -> Result<(), Error> {
        let from = self.pos.checked_sub(dist).ok_or(Error::LookBehind)?;
        if self.pos + len > self.buf.len() {
            return Err(Error::OutputOverrun);
        }
        for i in 0..len {
            self.buf[self.pos + i] = self.buf[from + i];
        }
        self.pos += len;
        Ok(())
    }
}

/// Input with bounds checks.
struct In<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl In<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self.buf.get(self.pos).ok_or(Error::InputOverrun)?;
        self.pos += 1;
        Ok(b)
    }

    /// A length extended by zero bytes: each 0 adds 255, and the first
    /// non-zero byte adds itself and `base`.
    fn long_length(&mut self, base: usize) -> Result<usize, Error> {
        let mut len = 0;
        loop {
            match self.byte()? {
                0 => len += 255,
                b => return Ok(len + base + usize::from(b)),
            }
        }
    }
}

// ANCHOR: nrv2b
/// UCL's NRV2B, 8-bit variant. Control bits come from bytes of the input,
/// most significant first, interleaved with the data bytes: a 1 bit means
/// a literal byte follows; a 0 bit starts a match, whose offset and length
/// are encoded in variable-length bit sequences (*gamma codes*: pairs of
/// a data bit and a "more" bit).
fn nrv2b(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let mut input = In { buf: src, pos: 0 };
    let mut out = Out { buf: dst, pos: 0 };
    // The bit buffer: a byte shifted left, with a 1 marking its end.
    let mut bb: u32 = 0;
    let mut bit = |input: &mut In| -> Result<u32, Error> {
        bb = if bb & 0x7f != 0 {
            bb * 2
        } else {
            u32::from(input.byte()?) * 2 + 1
        };
        Ok((bb >> 8) & 1)
    };
    let mut last_off: u32 = 1;
    loop {
        while bit(&mut input)? == 1 {
            let b = input.byte()?;
            out.push(b)?;
        }
        let mut off: u32 = 1;
        loop {
            off = off * 2 + bit(&mut input)?;
            if bit(&mut input)? == 1 {
                break;
            }
        }
        if off == 2 {
            off = last_off;
        } else {
            off = (off - 3)
                .wrapping_mul(256)
                .wrapping_add(u32::from(input.byte()?));
            if off == u32::MAX {
                return Ok(out.pos);
            }
            off += 1;
            last_off = off;
        }
        let mut len = bit(&mut input)?;
        len = len * 2 + bit(&mut input)?;
        if len == 0 {
            len = 1;
            loop {
                len = len * 2 + bit(&mut input)?;
                if bit(&mut input)? == 1 {
                    break;
                }
            }
            len += 2;
        }
        len += u32::from(off > 0xd00);
        out.copy_match(off as usize, len as usize + 1)?;
    }
}
// ANCHOR_END: nrv2b

// ANCHOR: lzo1x
/// LZO1X. The stream is byte-oriented: each *instruction* byte says what
/// follows. Small values start a run of literals; larger ones a match,
/// whose length and distance are packed into the instruction and the next
/// one or two bytes, in four encodings for near and far, short and long
/// matches. The low 2 bits of a match's last offset byte give 0 to 3
/// literals that follow it, saving an instruction.
fn lzo1x(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let mut input = In { buf: src, pos: 0 };
    let mut out = Out { buf: dst, pos: 0 };
    let literals = |input: &mut In, out: &mut Out, n: usize| -> Result<(), Error> {
        for _ in 0..n {
            let b = input.byte()?;
            out.push(b)?;
        }
        Ok(())
    };

    // What the next instruction byte may be: after a literal run of 4 or
    // more bytes, a small value is a 3-byte match rather than literals.
    #[derive(PartialEq)]
    enum After {
        Start,
        LongLiterals,
        Match,
    }
    let mut after = After::Start;
    let mut t = usize::from(input.byte()?);
    // A first byte above 17: a literal run of t - 17 bytes.
    if t > 17 {
        t -= 17;
        literals(&mut input, &mut out, t)?;
        after = if t < 4 {
            After::Match
        } else {
            After::LongLiterals
        };
        t = usize::from(input.byte()?);
    }
    loop {
        if t < 16 {
            match after {
                After::Start => {
                    // A literal run: 3 + t, or longer.
                    let n = if t == 0 { input.long_length(15)? } else { t };
                    literals(&mut input, &mut out, n + 3)?;
                    after = After::LongLiterals;
                    t = usize::from(input.byte()?);
                    continue;
                }
                After::LongLiterals => {
                    // A 3-byte match, 2049 to 3072 bytes back.
                    let dist = 1 + 0x800 + (t >> 2) + (usize::from(input.byte()?) << 2);
                    out.copy_match(dist, 3)?;
                }
                After::Match => {
                    // A 2-byte match, at most 1024 bytes back.
                    let dist = 1 + (t >> 2) + (usize::from(input.byte()?) << 2);
                    out.copy_match(dist, 2)?;
                }
            }
        } else if t >= 64 {
            // M2: 3 to 8 bytes, at most 2048 back.
            let dist = 1 + ((t >> 2) & 7) + (usize::from(input.byte()?) << 3);
            out.copy_match(dist, (t >> 5) + 1)?;
        } else if t >= 32 {
            // M3: any length, at most 16384 back.
            let len = if t & 31 == 0 {
                input.long_length(31)?
            } else {
                t & 31
            };
            let (lo, hi) = (usize::from(input.byte()?), usize::from(input.byte()?));
            out.copy_match(1 + (lo >> 2) + (hi << 6), len + 2)?;
        } else {
            // M4: any length, 16385 to 49151 back; distance 0 ends the
            // stream.
            let far = (t & 8) << 11;
            let len = if t & 7 == 0 {
                input.long_length(7)?
            } else {
                t & 7
            };
            let (lo, hi) = (usize::from(input.byte()?), usize::from(input.byte()?));
            let dist = far + (lo >> 2) + (hi << 6);
            if dist == 0 {
                return Ok(out.pos);
            }
            out.copy_match(dist + 0x4000, len + 2)?;
        }
        // The literals packed into the match's last byte.
        let n = usize::from(input.buf[input.pos - 2]) & 3;
        if n == 0 {
            after = After::Start;
        } else {
            literals(&mut input, &mut out, n)?;
            after = After::Match;
        }
        t = usize::from(input.byte()?);
    }
}
// ANCHOR_END: lzo1x

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;

    /// FNV-1a, to compare outputs without storing them.
    fn fnv(data: &[u8]) -> u64 {
        data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        })
    }

    // The last two blocks of image file systems compressed by mkifs, with
    // the length and hash of what the SDP's dumpifs decompressed them to.
    #[test]
    fn decompresses_mkifs_lzo_blocks() {
        let mut out = vec![0; 0x20000];
        let n = decompress(Algorithm::Lzo, include_bytes!("testdata/lzo.bin"), &mut out).unwrap();
        assert_eq!((n, fnv(&out[..n])), (67380, 0x8e23_22ba_ad30_3a02));
    }

    #[test]
    fn decompresses_mkifs_ucl_blocks() {
        let mut out = vec![0; 0x20000];
        let n = decompress(Algorithm::Ucl, include_bytes!("testdata/ucl.bin"), &mut out).unwrap();
        assert_eq!((n, fnv(&out[..n])), (67380, 0x41db_3154_9f4c_bad7));
    }

    #[test]
    fn reports_short_input_and_output() {
        let mut small = vec![0; 100];
        let data = include_bytes!("testdata/ucl.bin");
        assert_eq!(
            decompress(Algorithm::Ucl, data, &mut small),
            Err(Error::OutputOverrun)
        );
        let mut out = vec![0; 0x20000];
        assert_eq!(
            decompress(
                Algorithm::Lzo,
                &include_bytes!("testdata/lzo.bin")[..100],
                &mut out
            ),
            Err(Error::InputOverrun)
        );
    }
}
