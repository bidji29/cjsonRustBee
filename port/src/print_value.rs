//! The leaf `print_value`: cJSON's whole printer, as `cJSON_PrintPreallocated` runs it: a tree of items printed
//! into a buffer of a given length, formatted or not. The original cannot grow that buffer: `ensure` fails when
//! the bytes asked, the offset and one more exceed the length. What it asks is not always what it writes (a colon
//! is asked without a terminating zero, a number with one byte more), so this leaf does as the original, step by
//! step: the same calls to `ensure`, the same writes and the same offset, in a buffer of exactly that length (the
//! start of the answer's buffer, zeroed like the original's). It reads the tree's form as it prints, and keeps the
//! open arrays and objects in a table of its own rather than calling itself: the original refuses over 1000.
//! A number's text is that of the accepted leaf print_number, whose code is copied here, written shorter: the C
//! library converts exactly both ways ("%1.15g", "%lg", "%1.17g"), and so does this, on big integers.

const LIMBS: usize = 96;

/// An unsigned big integer, 32-bit limbs, the low one first: wide enough for every value used here (the largest
/// is a 53-bit significand times 5^1074, under 2^2548).
type Big = [u32; LIMBS];

fn big(v: u64) -> Big {
    let mut w = [0u32; LIMBS];
    (w[0], w[1]) = (v as u32, (v >> 32) as u32);
    w
}

fn is_zero(a: &Big) -> bool { a.iter().all(|&x| x == 0) }

fn bit_len(a: &Big) -> usize {
    (0..LIMBS).rev().find(|&i| a[i] != 0).map_or(0, |i| i * 32 + 32 - a[i].leading_zeros() as usize)
}

/// a *= base^k, base being 2, 5 or 10.
fn mul_pow(a: &mut Big, base: u32, k: u32) {
    let chunk = match base { 2 => 31, 5 => 13, _ => 9 };
    let mut left = k;
    while left > 0 {
        let step = left.min(chunk);
        let (f, mut carry) = (base.pow(step) as u64, 0u64);
        for x in a.iter_mut() {
            let t = *x as u64 * f + carry;
            (*x, carry) = (t as u32, t >> 32);
        }
        left -= step;
    }
}

/// a /= d, and the remainder.
fn div_small(a: &mut Big, d: u64) -> u32 {
    let mut rem = 0u64;
    for x in a.iter_mut().rev() {
        let cur = (rem << 32) | *x as u64;
        (*x, rem) = ((cur / d) as u32, cur % d);
    }
    rem as u32
}

/// When a >= o: a -= o, and true.
fn sub_if_ge(a: &mut Big, o: &Big) -> bool {
    let (mut t, mut borrow) = (*a, false);
    for i in 0..LIMBS {
        let (x, b1) = t[i].overflowing_sub(o[i]);
        let (y, b2) = x.overflowing_sub(borrow as u32);
        (t[i], borrow) = (y, b1 || b2);
    }
    if !borrow { *a = t; }
    !borrow
}

fn push(out: &mut [u8], n: &mut usize, text: &[u8]) {
    out[*n..*n + text.len()].copy_from_slice(text);
    *n += text.len();
}

/// The exact decimal digits of a finite, nonzero, positive double (its bits): the count of digits written at the
/// start of `digits`, and the decimal exponent of the first one.
fn decimal(bits: u64, digits: &mut [u8; 800]) -> (usize, i32) {
    let (frac, ex) = (bits & ((1u64 << 52) - 1), ((bits >> 52) & 0x7ff) as i32);
    let (m, e) = if ex == 0 { (frac, -1074) } else { (frac | (1u64 << 52), ex - 1075) };
    let mut n = big(m);
    // m * 2^e = m * 5^-e / 10^-e
    if e >= 0 { mul_pow(&mut n, 2, e as u32) } else { mul_pow(&mut n, 5, (-e) as u32) }
    let mut at = 800; // the digits, nine at a time from the last, written at the end (767 at most), then moved
    while !is_zero(&n) {
        let mut x = div_small(&mut n, 1_000_000_000);
        for _ in 0..9 {
            at -= 1;
            digits[at] = b'0' + (x % 10) as u8;
            x /= 10;
        }
    }
    while digits[at] == b'0' { at += 1; }
    digits.copy_within(at.., 0);
    (800 - at, (800 - at) as i32 - 1 + e.min(0))
}

/// The exact digits rounded to p significant digits (to the nearest, the even digit in a tie), and the decimal
/// exponent of the first digit after rounding.
fn round_to(digits: &[u8; 800], nd: usize, x0: i32, p: usize) -> ([u8; 17], i32) {
    let mut r = [b'0'; 17];
    let have = nd.min(p);
    r[..have].copy_from_slice(&digits[..have]);
    if nd <= p { return (r, x0); }
    let (next, odd) = (digits[p], (r[p - 1] - b'0') % 2 == 1);
    if next < b'5' || (next == b'5' && !odd && digits[p + 1..nd].iter().all(|&ch| ch == b'0')) {
        return (r, x0);
    }
    for i in (0..p).rev() {
        if r[i] != b'9' {
            r[i] += 1;
            return (r, x0);
        }
        r[i] = b'0';
    }
    r[0] = b'1'; // all nines: 10...0, one place higher
    (r, x0 + 1)
}

/// What "%1.<p>g" writes for the rounded digits r (p of them) and decimal exponent x.
fn format_g(neg: bool, r: &[u8; 17], p: usize, x: i32, out: &mut [u8]) -> usize {
    let mut n = 0;
    push(out, &mut n, if neg { b"-" } else { b"" });
    let mut last = p;
    while last > 0 && r[last - 1] == b'0' { last -= 1; }
    // the digits before the point, when the text has no exponent: "0.000…" stands before them all
    let ip = if x < -4 || x >= p as i32 { 1 } else if x < 0 { 0 } else { x as usize + 1 };
    if ip == 0 { push(out, &mut n, &b"0.000"[..(1 - x) as usize]); }
    push(out, &mut n, &r[..ip]);
    if last > ip {
        push(out, &mut n, if ip == 0 { b"" } else { b"." });
        push(out, &mut n, &r[ip..last]);
    }
    if x < -4 || x >= p as i32 {
        push(out, &mut n, if x < 0 { b"e-" } else { b"e+" });
        let a = x.unsigned_abs();
        if a >= 100 { push(out, &mut n, &[b'0' + (a / 100) as u8]); }
        push(out, &mut n, &[b'0' + (a / 10 % 10) as u8, b'0' + (a % 10) as u8]);
    }
    n
}

/// The double nearest to dm * 10^q (the even one in a tie; infinity past the largest), as strtod reads it.
fn nearest(dm: u64, q: i32) -> f64 {
    if dm == 0 { return 0.0; }
    let (mut num, mut den) = (big(dm), big(1));
    if q >= 0 { mul_pow(&mut num, 10, q as u32) } else { mul_pow(&mut den, 10, (-q) as u32) }
    let sh: i64 = 64 - (bit_len(&num) as i64 - bit_len(&den) as i64);
    if sh >= 0 { mul_pow(&mut num, 2, sh as u32) } else { mul_pow(&mut den, 2, (-sh) as u32) }
    // num / den now lies in [2^63, 2^65): its integer part, bit by bit, from the bit 65.
    mul_pow(&mut den, 2, 65);
    let mut quo: u128 = 0;
    for _ in 0..66 {
        quo = quo << 1 | sub_if_ge(&mut num, &den) as u128;
        mul_pow(&mut num, 2, 1);
    }
    let sticky = !is_zero(&num);
    // The value is (quo + a fraction, nonzero when sticky) * 2^-sh.
    let l = 128 - quo.leading_zeros() as i64;
    let mut e = l - 1 - sh;
    let drop: i64 = if e >= -1022 { l - 53 } else { sh - 1074 };
    if drop >= 128 { return 0.0; }
    let drop = drop as u32;
    let (kept, rem, half) = (quo >> drop, quo & ((1u128 << drop) - 1), 1u128 << (drop - 1));
    let mut kept = kept + (rem > half || (rem == half && (sticky || kept & 1 == 1))) as u128;
    // a subnormal; rounded up to 2^52, it is the smallest normal, whose bits these are too
    if e < -1022 { return f64::from_bits(kept as u64); }
    if kept == 1u128 << 53 {
        kept >>= 1;
        e += 1;
    }
    if e > 1023 { return f64::INFINITY; }
    f64::from_bits((((e + 1023) as u64) << 52) | (kept as u64 - (1u64 << 52)))
}

fn fabs(x: f64) -> f64 { f64::from_bits(x.to_bits() & !(1u64 << 63)) }

fn compare_double(a: f64, b: f64) -> bool {
    let max_val = if fabs(a) > fabs(b) { fabs(a) } else { fabs(b) };
    fabs(a - b) <= max_val * f64::EPSILON
}

/// The text `print_number` writes for an item's valuedouble (its bits) and valueint: its length.
fn number_text(bits: u64, vi: i32, out: &mut [u8; 26]) -> usize {
    let (d, mut n) = (f64::from_bits(bits), 0);
    let (neg, mag) = (bits >> 63 == 1, bits & !(1u64 << 63));
    if d.is_nan() || d.is_infinite() {
        push(out, &mut n, b"null");
    } else if d == vi as f64 {
        // "%d"
        push(out, &mut n, if vi < 0 { b"-" } else { b"" });
        let (mut v, mut tmp, mut t) = (vi.unsigned_abs(), [0u8; 10], 10);
        while t == 10 || v > 0 {
            t -= 1;
            tmp[t] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        push(out, &mut n, &tmp[t..]);
    } else if mag == 0 {
        // a zero whose valueint is not 0: "%1.15g" writes it, and it reads back equal
        push(out, &mut n, if neg { b"-0" } else { b"0" });
    } else {
        let mut digits = [0u8; 800];
        let (nd, x0) = decimal(mag, &mut digits);
        let (r15, x15) = round_to(&digits, nd, x0, 15);
        let dm = r15[..15].iter().fold(0u64, |dm, &ch| dm * 10 + (ch - b'0') as u64);
        let t = nearest(dm, x15 - 14);
        if compare_double(if neg { -t } else { t }, d) { return format_g(neg, &r15, 15, x15, out); }
        let (r17, x17) = round_to(&digits, nd, x0, 17);
        return format_g(neg, &r17, 17, x17, out);
    }
    n
}

const NESTING_LIMIT: usize = 1000;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The tree's form, read from its start.
struct Reader<'a> { bytes: &'a [u8], at: usize }

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ()> {
        let part = self.bytes[self.at..].get(..count).ok_or(())?;
        self.at += count;
        Ok(part)
    }

    fn u16(&mut self) -> Result<usize, ()> {
        let p = self.take(2)?;
        Ok(p[0] as usize | (p[1] as usize) << 8)
    }

    /// A text: `None` is NULL; one that holds a zero byte is refused.
    fn text(&mut self) -> Result<Option<&'a [u8]>, ()> {
        let length = self.u16()?;
        if length == 0xFFFF { return Ok(None); }
        let t = self.take(length)?;
        if t.contains(&0) { Err(()) } else { Ok(Some(t)) }
    }
}

/// The original's printbuffer: `buf` is the buffer, of exactly the length given.
struct Printer<'a> { buf: &'a mut [u8], offset: usize, format: bool }

impl<'a> Printer<'a> {
    /// `ensure`: the offset, to write at, when `needed` bytes, the offset and one more fit in the length.
    fn ensure(&self, needed: usize) -> Result<usize, ()> {
        if needed + self.offset + 1 <= self.buf.len() { Ok(self.offset) } else { Err(()) }
    }

    /// `update_offset`: the offset moves to the zero that ends the text written at it.
    fn update_offset(&mut self) -> Result<(), ()> {
        self.offset += self.buf.get(self.offset..).ok_or(())?.iter().position(|&b| b == 0).ok_or(())?;
        Ok(())
    }

    /// After `ensure(asked)`, writes `text` at the offset, which moves by `advance`.
    fn put(&mut self, asked: usize, text: &[u8], advance: usize) -> Result<(), ()> {
        let mut at = self.ensure(asked)?;
        push(self.buf, &mut at, text);
        self.offset += advance;
        Ok(())
    }

    /// After `ensure(asked)`, writes `tabs` tabs and `text` at the offset, which moves by `advance`.
    fn tabs(&mut self, asked: usize, tabs: usize, text: &[u8], advance: usize) -> Result<(), ()> {
        let at = self.ensure(asked)?;
        self.buf[at..at + tabs].fill(b'\t');
        self.offset += tabs;
        self.put(0, text, 0)?; // the room is ensured already: this cannot fail
        self.offset = at + advance;
        Ok(())
    }

    /// `print_string_ptr`: the offset stays.
    fn string(&mut self, text: Option<&[u8]>) -> Result<(), ()> {
        let Some(input) = text else { return self.put(3, b"\"\"\0", 0) };
        // the letter of a short escape, or 0
        let short = |b: u8| match b {
            b'"' | b'\\' => b,
            0x08 => b'b', 0x0c => b'f', b'\n' => b'n', b'\r' => b'r', b'\t' => b't',
            _ => 0,
        };
        let escapes: usize = input.iter().map(|&b| if short(b) != 0 { 1 } else if b < 32 { 5 } else { 0 }).sum();
        let mut at = self.ensure(input.len() + escapes + 3)?;
        push(self.buf, &mut at, b"\"");
        for &b in input {
            if short(b) != 0 {
                push(self.buf, &mut at, &[b'\\', short(b)]);
            } else if b < 32 {
                push(self.buf, &mut at, &[b'\\', b'u', b'0', b'0', HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]]);
            } else {
                push(self.buf, &mut at, &[b]);
            }
        }
        push(self.buf, &mut at, b"\"\0");
        Ok(())
    }

    /// `print_number`, given the twelve bytes of the item: the offset moves past the text.
    fn number(&mut self, item: &[u8]) -> Result<(), ()> {
        let (mut bits, mut text) = ([0u8; 8], [0u8; 26]);
        bits.copy_from_slice(&item[..8]);
        let vi = i32::from_le_bytes([item[8], item[9], item[10], item[11]]);
        let n = number_text(u64::from_le_bytes(bits), vi, &mut text);
        self.put(n + 1, &text[..n + 1], n)
    }
}

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 3 { return Err(()); }
    let length = input[0] as usize | (input[1] as usize) << 8;
    if length > 6 * input.len() + 16 { return Err(()); }
    let mut p = Printer { buf: out.get_mut(..length).ok_or(())?, offset: 0, format: input[2] != 0 };
    let format = p.format;
    let mut r = Reader { bytes: input, at: 3 };
    // The open arrays and objects, the outermost first: the children each has still to print, and whether it is
    // an object. `depth` is their count: the original's `depth`.
    let (mut left, mut named, mut depth) = ([0usize; NESTING_LIMIT], [false; NESTING_LIMIT], 0usize);
    let mut kind = r.take(1)?[0];
    loop {
        // print_value, of an item of the type `kind` whose name, if it has one, is read (and printed) already
        let mut printed = true;
        match kind {
            4 => p.put(5, b"null\0", 0)?,
            1 => p.put(6, b"false\0", 0)?,
            2 => p.put(5, b"true\0", 0)?,
            8 => p.number(r.take(12)?)?,
            16 => p.string(r.text()?)?,
            128 => {
                let raw = r.text()?.ok_or(())?;
                p.put(raw.len() + 1, raw, raw.len())?;
                p.put(0, b"\0", 0)?; // the room is ensured already: this cannot fail
                p.offset -= raw.len();
            }
            32 | 64 => {
                let count = r.u16()?;
                if depth >= NESTING_LIMIT { return Err(()); }
                match (kind == 64, format) {
                    (false, _) => p.put(1, b"[", 1)?,
                    (true, false) => p.put(2, b"{", 1)?,
                    (true, true) => p.put(3, b"{\n", 2)?,
                }
                (left[depth], named[depth], printed) = (count, kind == 64, false);
                depth += 1;
            }
            _ => return Err(()),
        }
        // What the array or the object the item lies in does next, until it has a child to print.
        loop {
            if printed {
                if depth == 0 {
                    // the item given is printed: the answer is the text, to its zero
                    return if r.at == input.len() { p.buf.iter().position(|&b| b == 0).ok_or(()) } else { Err(()) };
                }
                p.update_offset()?;
                match (named[depth - 1], left[depth - 1] > 0, format) {
                    (false, false, _) => {}
                    (false, true, false) | (true, true, false) => p.put(2, b",\0", 1)?,
                    (false, true, true) => p.put(3, b", \0", 2)?,
                    (true, true, true) => p.put(3, b",\n\0", 2)?,
                    (true, false, true) => p.put(2, b"\n\0", 1)?,
                    (true, false, false) => p.put(1, b"\0", 0)?,
                }
            }
            let top = depth - 1;
            if left[top] > 0 {
                left[top] -= 1;
                kind = r.take(1)?[0];
                if named[top] {
                    let name = r.text()?;
                    if format { p.tabs(depth, depth, b"", depth)?; }
                    p.string(name)?;
                    p.update_offset()?;
                    if format { p.put(2, b":\t", 2)? } else { p.put(1, b":", 1)? }
                }
                break;
            }
            // no child left: the array or the object closes, and is itself an item printed
            match (named[top], format) {
                (false, _) => p.put(2, b"]\0", 0)?,
                (true, false) => p.put(2, b"}\0", 0)?,
                (true, true) => p.tabs(depth + 1, depth - 1, b"}\0", 0)?,
            }
            (depth, printed) = (top, true);
        }
    }
}
