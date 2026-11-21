//! The leaf `parse_number`: the text of a JSON number read as a number, as cJSON's `parse_number`.
//!
//! The original copies the longest run of `0-9 + - e E .` at the start of its input and hands it to the C
//! library's `strtod`, whose answer is the specification: the decimal text rounded to the nearest double (the
//! even one in a tie). It is refused when `strtod` reads nothing. It answers fourteen bytes: the count of bytes
//! read (two bytes, low first), the double's eight bytes (low first), then `valueint` (four bytes, low first),
//! saturated to the range of a 32-bit int as the original does.

use core::cmp::Ordering;

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if out.len() < 14 {
        return Err(());
    }
    let number = read(input).ok_or(())?;
    let value = f64::from_bits(number.bits);
    let int: i32 = if value >= 2147483647.0 {
        i32::MAX
    } else if value <= -2147483648.0 {
        i32::MIN
    } else {
        value as i32
    };
    let count = number.count as u16;
    out[..2].copy_from_slice(&count.to_le_bytes());
    out[2..10].copy_from_slice(&number.bits.to_le_bytes());
    out[10..14].copy_from_slice(&int.to_le_bytes());
    Ok(14)
}

/// What `strtod` answers: the bytes it read and the double's pattern.
struct Number {
    count: usize,
    bits: u64,
}

const INFINITY: u64 = 0x7FF0_0000_0000_0000;
const FRACTION: u64 = (1 << 52) - 1;
const SIGN: u64 = 1 << 63;

/// The grammar `strtod` reads in the bytes the original copies: an optional sign, digits with at most one point
/// (at least one digit), then an exponent only when a digit follows its `e` (and its sign). Every other byte of
/// `strtod`'s grammar (blanks, `x`, `inf`, `nan`) is not one the original copies.
fn read(input: &[u8]) -> Option<Number> {
    let at = |i: usize| -> u8 { if i < input.len() { input[i] } else { 0 } };
    let mut i = 0;
    let negative = at(0) == b'-';
    if at(0) == b'-' || at(0) == b'+' {
        i += 1;
    }
    // The digits, as they come: the first one that is not zero starts them; those after the point count down
    // the exponent.
    let digits_start = i;
    let mut seen_digit = false;
    let mut point = false;
    let mut exponent: i64 = 0;
    loop {
        let c = at(i);
        if c.is_ascii_digit() {
            seen_digit = true;
            if point {
                exponent -= 1;
            }
            i += 1;
        } else if c == b'.' && !point {
            point = true;
            i += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        return None;
    }
    let digits_end = i;
    if at(i) == b'e' || at(i) == b'E' {
        let mut j = i + 1;
        let exponent_negative = at(j) == b'-';
        if at(j) == b'-' || at(j) == b'+' {
            j += 1;
        }
        if at(j).is_ascii_digit() {
            let mut written: i64 = 0;
            while at(j).is_ascii_digit() {
                if written < 100_000_000 {
                    written = written * 10 + i64::from(at(j) - b'0');
                }
                j += 1;
            }
            exponent += if exponent_negative { -written } else { written };
            i = j;
        }
    }
    let magnitude = decimal_to_double(&input[digits_start..digits_end], exponent);
    Some(Number { count: i, bits: if negative { magnitude | SIGN } else { magnitude } })
}

/// The digits of the text, without the point, the leading zeros and the trailing zeros, and the power of ten
/// they are multiplied by.
struct Decimal<'a> {
    text: &'a [u8],
    count: i64,
    exponent: i64,
}

impl<'a> Decimal<'a> {
    fn digits(&self) -> impl Iterator<Item = u8> + 'a {
        self.text.iter().copied().filter(|c| *c != b'.').map(|c| c - b'0')
    }
}

/// The double nearest to the digits (with their point) times ten to `exponent`, positive.
fn decimal_to_double(text: &[u8], exponent: i64) -> u64 {
    // Take off the leading zeros (and the point among them), then the trailing zeros: the exponent already
    // counted the digits after the point, so each trailing zero taken off is one more power of ten.
    let mut start = 0;
    while start < text.len() && (text[start] == b'0' || text[start] == b'.') {
        start += 1;
    }
    let mut end = text.len();
    let mut exponent = exponent;
    while end > start && (text[end - 1] == b'0' || text[end - 1] == b'.') {
        if text[end - 1] == b'0' {
            exponent += 1;
        }
        end -= 1;
    }
    let kept = &text[start..end];
    let count = kept.iter().filter(|c| **c != b'.').count() as i64;
    if count == 0 {
        return 0;
    }
    let decimal = Decimal { text: kept, count, exponent };
    // Beyond these, the answer is infinity or zero whatever the digits are.
    if decimal.count + decimal.exponent > 310 {
        return INFINITY;
    }
    if decimal.count + decimal.exponent < -324 {
        return 0;
    }
    let mut candidate = estimate(&decimal);
    // Move the candidate until the value lies between the two midpoints around it (the even one on a tie).
    loop {
        if candidate != INFINITY {
            let order = compare(&decimal, midpoint(candidate));
            if order == Ordering::Greater || (order == Ordering::Equal && candidate & 1 == 1) {
                candidate += 1;
                continue;
            }
        }
        if candidate != 0 {
            let order = compare(&decimal, midpoint(candidate - 1));
            if order == Ordering::Less || (order == Ordering::Equal && candidate & 1 == 1) {
                candidate -= 1;
                continue;
            }
        }
        return candidate;
    }
}

/// The midpoint between the finite double `bits` (positive) and the next one up: `m` times two to `e`.
fn midpoint(bits: u64) -> (u64, i64) {
    let biased = (bits >> 52) as i64;
    let fraction = bits & FRACTION;
    let (significand, power) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };
    (2 * significand + 1, power - 1)
}

/// A first guess, within a few units of the last place: the first nineteen digits, multiplied or divided by ten
/// with 64 bits kept at each step.
fn estimate(decimal: &Decimal) -> u64 {
    let mut top: u64 = 0;
    let mut used: i64 = 0;
    for d in decimal.digits() {
        if used == 19 {
            break;
        }
        top = top * 10 + u64::from(d);
        used += 1;
    }
    let shift = top.leading_zeros();
    let mut m = top << shift;
    let mut b: i64 = -i64::from(shift);
    let mut tens = decimal.exponent + (decimal.count - used);
    while tens > 0 {
        let p = u128::from(m) * 10;
        let extra = 64 - p.leading_zeros();
        m = (p >> extra) as u64;
        b += i64::from(extra);
        tens -= 1;
    }
    while tens < 0 {
        let q = (u128::from(m) << 64) / 10;
        let extra = 64 - q.leading_zeros();
        m = (q >> extra) as u64;
        b += i64::from(extra) - 64;
        tens += 1;
    }
    let biased = b + 63 + 1023;
    if biased >= 2047 {
        INFINITY
    } else if biased >= 1 {
        ((biased as u64) << 52) | ((m >> 11) & FRACTION)
    } else {
        let shift = 12 - biased;
        if shift >= 64 {
            0
        } else {
            m >> shift
        }
    }
}

/// The exact order of the decimal and `m` times two to `e`, in integers.
fn compare(decimal: &Decimal, (m, e): (u64, i64)) -> Ordering {
    let mut left = Big::zero();
    for d in decimal.digits() {
        left.mul_small(10);
        left.add_small(u32::from(d));
    }
    let mut right = Big::zero();
    right.add_small(m as u32);
    right.add_high((m >> 32) as u32);
    if decimal.exponent >= 0 {
        left.mul_pow10(decimal.exponent as u32);
    } else {
        right.mul_pow10((-decimal.exponent) as u32);
    }
    if e >= 0 {
        right.shl(e as u32);
    } else {
        left.shl((-e) as u32);
    }
    left.cmp(&right)
}

const LIMBS: usize = 96;

/// An unsigned integer of up to 96 times 32 bits, low limb first.
struct Big {
    limbs: [u32; LIMBS],
}

impl Big {
    fn zero() -> Big {
        Big { limbs: [0; LIMBS] }
    }

    fn mul_small(&mut self, factor: u32) {
        let mut carry: u64 = 0;
        for limb in self.limbs.iter_mut() {
            let p = u64::from(*limb) * u64::from(factor) + carry;
            *limb = p as u32;
            carry = p >> 32;
        }
    }

    fn add_small(&mut self, value: u32) {
        let mut carry = u64::from(value);
        for limb in self.limbs.iter_mut() {
            if carry == 0 {
                break;
            }
            let s = u64::from(*limb) + carry;
            *limb = s as u32;
            carry = s >> 32;
        }
    }

    /// Adds `value` times two to the 32.
    fn add_high(&mut self, value: u32) {
        let mut carry = u64::from(value);
        for limb in self.limbs[1..].iter_mut() {
            if carry == 0 {
                break;
            }
            let s = u64::from(*limb) + carry;
            *limb = s as u32;
            carry = s >> 32;
        }
    }

    fn mul_pow10(&mut self, mut power: u32) {
        while power >= 9 {
            self.mul_small(1_000_000_000);
            power -= 9;
        }
        let mut rest: u32 = 1;
        while power > 0 {
            rest *= 10;
            power -= 1;
        }
        self.mul_small(rest);
    }

    fn shl(&mut self, bits: u32) {
        let whole = (bits / 32) as usize;
        let part = bits % 32;
        if whole > 0 {
            let mut i = LIMBS;
            while i > 0 {
                i -= 1;
                self.limbs[i] = if i >= whole { self.limbs[i - whole] } else { 0 };
            }
        }
        if part > 0 {
            let mut i = LIMBS;
            while i > 0 {
                i -= 1;
                let low = if i > 0 { self.limbs[i - 1] >> (32 - part) } else { 0 };
                self.limbs[i] = (self.limbs[i] << part) | low;
            }
        }
    }

    fn cmp(&self, other: &Big) -> Ordering {
        let mut i = LIMBS;
        while i > 0 {
            i -= 1;
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => {}
                order => return order,
            }
        }
        Ordering::Equal
    }
}
