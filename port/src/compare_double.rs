//! The leaf `compare_double`: whether two doubles count as equal, as cJSON's `compare_double`.
//!
//! Given exactly sixteen bytes, two doubles `a` then `b` (each its IEEE 754 pattern, low byte first), it answers
//! one byte: 1 when the original says they are equal, else 0. Any other length is refused.
//!
//! The original: `maxVal = fabs(a) > fabs(b) ? fabs(a) : fabs(b)`, then `fabs(a - b) <= maxVal * DBL_EPSILON`.
//! Every step is an IEEE operation on doubles, so the same steps here give the same answer, for NaNs and
//! infinities too (a NaN compares false; an infinity against a finite number counts as equal).

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() != 16 || out.is_empty() {
        return Err(());
    }
    let a = read_double(&input[0..8]);
    let b = read_double(&input[8..16]);
    out[0] = u8::from(equal(a, b));
    Ok(1)
}

/// Eight bytes, low byte first, as the double whose pattern they are.
fn read_double(bytes: &[u8]) -> f64 {
    let mut pattern = [0u8; 8];
    pattern.copy_from_slice(bytes);
    f64::from_bits(u64::from_le_bytes(pattern))
}

/// `fabs`: the same pattern with its sign bit cleared (NaNs keep their payload).
fn absolute(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

/// The original's two lines.
fn equal(a: f64, b: f64) -> bool {
    let abs_a = absolute(a);
    let abs_b = absolute(b);
    let max_val = if abs_a > abs_b { abs_a } else { abs_b };
    absolute(a - b) <= max_val * f64::EPSILON
}
