//! The leaf `hex4`: four hexadecimal digits to a number, as cJSON's `parse_hex4`.
//!
//! Given exactly four bytes, it answers two bytes: the number, high byte first. Any byte that is not a
//! hexadecimal digit makes the whole number 0, as in the original. Any other length is refused.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() != 4 || out.len() < 2 {
        return Err(());
    }
    let value = number(input);
    out[0] = (value >> 8) as u8;
    out[1] = value as u8;
    Ok(2)
}

/// The original's loop: each digit is added, then the sum is shifted to make place for the next one.
fn number(digits: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for (i, &byte) in digits.iter().enumerate() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'F' => 10 + (byte - b'A'),
            b'a'..=b'f' => 10 + (byte - b'a'),
            _ => return 0,
        };
        h += u32::from(digit);
        if i < 3 {
            h <<= 4;
        }
    }
    h
}
