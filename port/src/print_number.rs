//! The leaf `print_number`: cJSON's print_number, an item's number written as JSON text.
//!
//! The original writes "null" for NaN and the infinities, the integer for a double equal to the item's
//! valueint, and otherwise the double with "%1.15g"; it reads that text back ("%lg") and keeps it when
//! compare_double finds it equal to the double, else it writes the double with "%1.17g". The C library
//! converts exactly both ways; so does this leaf, on big integers of fixed size.

const LIMBS: usize = 96;

/// An unsigned big integer, little-endian 32-bit limbs, wide enough for every value used here
/// (the largest is a 53-bit significand times 5^1074, under 2^2548).
struct Big {
    w: [u32; LIMBS],
}

impl Clone for Big {
    fn clone(&self) -> Big { Big { w: self.w } }
}

impl Copy for Big {}

impl Big {
    fn from_u64(v: u64) -> Big {
        let mut w = [0u32; LIMBS];
        w[0] = v as u32;
        w[1] = (v >> 32) as u32;
        Big { w }
    }

    fn is_zero(&self) -> bool { self.w.iter().all(|&x| x == 0) }

    fn mul_small(&mut self, m: u32) {
        let mut carry: u64 = 0;
        for x in self.w.iter_mut() {
            let t = (*x as u64) * (m as u64) + carry;
            *x = t as u32;
            carry = t >> 32;
        }
    }

    /// Multiplies by base^k, base being 5 or 10.
    fn mul_pow(&mut self, base: u32, k: u32) {
        let chunk = if base == 5 { 13 } else { 9 };
        let mut left = k;
        while left > 0 {
            let step = if left < chunk { left } else { chunk };
            let mut f: u32 = 1;
            for _ in 0..step {
                f *= base;
            }
            self.mul_small(f);
            left -= step;
        }
    }

    /// Divides by d in place, answers the remainder.
    fn div_small(&mut self, d: u32) -> u32 {
        let mut rem: u64 = 0;
        for x in self.w.iter_mut().rev() {
            let cur = (rem << 32) | (*x as u64);
            *x = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
        rem as u32
    }

    fn bit_len(&self) -> usize {
        for i in (0..LIMBS).rev() {
            if self.w[i] != 0 {
                return i * 32 + 32 - self.w[i].leading_zeros() as usize;
            }
        }
        0
    }

    fn shl(&mut self, s: usize) {
        let ws = s / 32;
        let bs = (s % 32) as u32;
        for i in (0..LIMBS).rev() {
            let mut v = 0u32;
            if i >= ws {
                let j = i - ws;
                v = self.w[j] << bs;
                if bs > 0 && j > 0 {
                    v |= self.w[j - 1] >> (32 - bs);
                }
            }
            self.w[i] = v;
        }
    }

    fn shr1(&mut self) {
        for i in 0..LIMBS {
            self.w[i] = (self.w[i] >> 1) | if i + 1 < LIMBS { self.w[i + 1] << 31 } else { 0 };
        }
    }

    fn ge(&self, o: &Big) -> bool {
        for i in (0..LIMBS).rev() {
            if self.w[i] != o.w[i] {
                return self.w[i] > o.w[i];
            }
        }
        true
    }

    /// self -= o, with self >= o.
    fn sub(&mut self, o: &Big) {
        let mut borrow = 0u64;
        for i in 0..LIMBS {
            let (a, b) = (self.w[i] as u64 + (1u64 << 32), o.w[i] as u64 + borrow);
            self.w[i] = (a - b) as u32;
            borrow = if a - b < (1u64 << 32) { 1 } else { 0 };
        }
    }
}

/// The exact decimal digits of a finite, nonzero, positive double (its bits): the count of digits
/// written into `digits`, and the decimal exponent of the first one.
fn decimal(bits: u64, digits: &mut [u8; 800]) -> (usize, i32) {
    let frac = bits & ((1u64 << 52) - 1);
    let ex = ((bits >> 52) & 0x7ff) as i32;
    let (m, e) = if ex == 0 { (frac, -1074) } else { (frac | (1u64 << 52), ex - 1075) };
    let mut n = Big::from_u64(m);
    let k;
    if e >= 0 {
        n.shl(e as usize);
        k = 0;
    } else {
        // m * 2^e = m * 5^-e / 10^-e
        n.mul_pow(5, (-e) as u32);
        k = e;
    }
    let mut chunks = [0u32; 96];
    let mut c = 0;
    while !n.is_zero() {
        chunks[c] = n.div_small(1_000_000_000);
        c += 1;
    }
    let mut nd = 0;
    for idx in (0..c).rev() {
        let mut tmp = [0u8; 9];
        let mut x = chunks[idx];
        for t in (0..9).rev() {
            tmp[t] = b'0' + (x % 10) as u8;
            x /= 10;
        }
        let mut start = 0;
        while idx == c - 1 && start < 8 && tmp[start] == b'0' {
            start += 1;
        }
        push(digits, &mut nd, &tmp[start..]);
    }
    (nd, nd as i32 - 1 + k)
}

/// The exact digits rounded to p significant digits (to the nearest, the even digit in a tie), and the
/// decimal exponent of the first digit after rounding.
fn round_to(digits: &[u8; 800], nd: usize, x0: i32, p: usize) -> ([u8; 17], i32) {
    let mut r = [b'0'; 17];
    let have = if nd < p { nd } else { p };
    r[..have].copy_from_slice(&digits[..have]);
    let mut x = x0;
    if nd > p {
        let next = digits[p];
        let up = next > b'5'
            || (next == b'5' && (digits[p + 1..nd].iter().any(|&ch| ch != b'0') || (r[p - 1] - b'0') % 2 == 1));
        if up {
            let mut i = p;
            loop {
                if i == 0 {
                    r[0] = b'1'; // all nines: 10...0, one place higher
                    x += 1;
                    break;
                }
                i -= 1;
                if r[i] == b'9' {
                    r[i] = b'0';
                } else {
                    r[i] += 1;
                    break;
                }
            }
        }
    }
    (r, x)
}

/// What "%1.<p>g" writes for the rounded digits r (p of them) and decimal exponent x.
fn format_g(neg: bool, r: &[u8; 17], p: usize, x: i32, out: &mut [u8]) -> usize {
    let mut n = 0;
    push(out, &mut n, if neg { b"-" } else { b"" });
    let mut last = p;
    while last > 0 && r[last - 1] == b'0' {
        last -= 1;
    }
    if x < p as i32 && x >= -4 {
        if x >= 0 {
            let ip = x as usize + 1;
            push(out, &mut n, &r[..ip]);
            if last > ip {
                push(out, &mut n, b".");
                push(out, &mut n, &r[ip..last]);
            }
        } else {
            push(out, &mut n, b"0.");
            for _ in 0..(-x - 1) {
                push(out, &mut n, b"0");
            }
            push(out, &mut n, &r[..last]);
        }
    } else {
        push(out, &mut n, &r[..1]);
        if last > 1 {
            push(out, &mut n, b".");
            push(out, &mut n, &r[1..last]);
        }
        push(out, &mut n, if x < 0 { b"e-" } else { b"e+" });
        let a = if x < 0 { -x } else { x } as u32;
        if a >= 100 {
            out[n] = b'0' + (a / 100) as u8;
            n += 1;
        }
        out[n] = b'0' + (a / 10 % 10) as u8;
        out[n + 1] = b'0' + (a % 10) as u8;
        n += 2;
    }
    n
}

/// The double nearest to dm * 10^q (the even one in a tie; infinity past the largest), as strtod reads it.
fn nearest(dm: u64, q: i32) -> f64 {
    if dm == 0 {
        return 0.0;
    }
    let mut num = Big::from_u64(dm);
    let mut den = Big::from_u64(1);
    if q >= 0 {
        num.mul_pow(10, q as u32);
    } else {
        den.mul_pow(10, (-q) as u32);
    }
    let b = num.bit_len() as i64 - den.bit_len() as i64;
    let sh: i64 = 64 - b;
    if sh >= 0 {
        num.shl(sh as usize);
    } else {
        den.shl((-sh) as usize);
    }
    // num / den now lies in [2^63, 2^65): its integer part, bit by bit.
    let mut dd = den;
    dd.shl(65);
    let mut quo: u128 = 0;
    let mut i = 65;
    loop {
        if num.ge(&dd) {
            num.sub(&dd);
            quo |= 1u128 << i;
        }
        if i == 0 {
            break;
        }
        i -= 1;
        dd.shr1();
    }
    let sticky = !num.is_zero();
    // The value is (quo + a fraction, nonzero when sticky) * 2^-sh.
    let l = 128 - quo.leading_zeros() as i64;
    let e = l - 1 - sh;
    let drop: i64 = if e >= -1022 { l - 53 } else { sh - 1074 };
    if drop >= 128 {
        return 0.0;
    }
    let drop = drop as u32;
    let kept = quo >> drop;
    let rem = quo & ((1u128 << drop) - 1);
    let half = 1u128 << (drop - 1);
    let up = rem > half || (rem == half && (sticky || kept & 1 == 1));
    let mut kept = kept + up as u128;
    let bits;
    if e >= -1022 {
        let mut e = e;
        if kept == 1u128 << 53 {
            kept >>= 1;
            e += 1;
        }
        if e > 1023 {
            return f64::INFINITY;
        }
        bits = (((e + 1023) as u64) << 52) | (kept as u64 - (1u64 << 52));
    } else {
        // a subnormal; rounded up to 2^52, it is the smallest normal, whose bits these are too
        bits = kept as u64;
    }
    f64::from_bits(bits)
}

fn fabs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

fn compare_double(a: f64, b: f64) -> bool {
    let fa = fabs(a);
    let fb = fabs(b);
    let max_val = if fa > fb { fa } else { fb };
    fabs(a - b) <= max_val * f64::EPSILON
}

fn push(out: &mut [u8], n: &mut usize, text: &[u8]) {
    out[*n..*n + text.len()].copy_from_slice(text);
    *n += text.len();
}

fn put(out: &mut [u8], text: &[u8]) -> usize {
    let mut n = 0;
    push(out, &mut n, text);
    n
}

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() != 12 {
        return Err(());
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&input[..8]);
    let bits = u64::from_le_bytes(b);
    let d = f64::from_bits(bits);
    let vi = i32::from_le_bytes([input[8], input[9], input[10], input[11]]);

    if d.is_nan() || d.is_infinite() {
        return Ok(put(out, b"null"));
    }
    if d == vi as f64 {
        // "%d"
        let mut n = 0;
        push(out, &mut n, if vi < 0 { b"-" } else { b"" });
        let mut v = (vi as i64).unsigned_abs();
        let mut tmp = [0u8; 10];
        let mut t = 10;
        while t == 10 || v > 0 {
            t -= 1;
            tmp[t] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        push(out, &mut n, &tmp[t..]);
        return Ok(n);
    }

    let neg = bits >> 63 == 1;
    let mag = bits & !(1u64 << 63);
    if mag == 0 {
        // a zero whose valueint is not 0: "%1.15g" writes it, and it reads back equal
        return Ok(put(out, if neg { b"-0" } else { b"0" }));
    }

    let mut digits = [0u8; 800];
    let (nd, x0) = decimal(mag, &mut digits);
    let (r15, x15) = round_to(&digits, nd, x0, 15);
    let mut dm: u64 = 0;
    for &ch in &r15[..15] {
        dm = dm * 10 + (ch - b'0') as u64;
    }
    let t = nearest(dm, x15 - 14);
    let test = if neg { -t } else { t };
    if compare_double(test, d) {
        Ok(format_g(neg, &r15, 15, x15, out))
    } else {
        let (r17, x17) = round_to(&digits, nd, x0, 17);
        Ok(format_g(neg, &r17, 17, x17, out))
    }
}
