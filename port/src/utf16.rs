//! The leaf `utf16`: a `\uXXXX` escape, or a surrogate pair of two, to UTF-8, as cJSON's
//! `utf16_literal_to_utf8`.
//!
//! Given the text of the escape and what follows, it answers one byte, the count of input bytes read (6 or 12),
//! then the UTF-8 bytes. As in the original, the first two bytes of the first escape are not looked at (the
//! caller has seen them), while those of the second must be a backslash and `u`; digits that are not
//! hexadecimal make the number 0, which is written as one zero byte.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 6 {
        return Err(());
    }
    let first_code = number(&input[2..6]);

    // A low surrogate cannot come first.
    if (0xDC00..=0xDFFF).contains(&first_code) {
        return Err(());
    }

    let (read, codepoint): (u8, u32) = if (0xD800..=0xDBFF).contains(&first_code) {
        // A high surrogate: the second escape must follow, and hold a low surrogate.
        if input.len() < 12 {
            return Err(());
        }
        if input[6] != b'\\' || input[7] != b'u' {
            return Err(());
        }
        let second_code = number(&input[8..12]);
        if !(0xDC00..=0xDFFF).contains(&second_code) {
            return Err(());
        }
        (12, 0x10000 + (((first_code & 0x3FF) << 10) | (second_code & 0x3FF)))
    } else {
        (6, first_code)
    };

    let (length, first_byte_mark): (usize, u32) = if codepoint < 0x80 {
        (1, 0)
    } else if codepoint < 0x800 {
        (2, 0xC0)
    } else if codepoint < 0x10000 {
        (3, 0xE0)
    } else if codepoint <= 0x10FFFF {
        (4, 0xF0)
    } else {
        return Err(());
    };

    if out.len() < 1 + length {
        return Err(());
    }
    out[0] = read;
    let written = &mut out[1..1 + length];

    // The continuation bytes, last first: 10xxxxxx.
    let mut rest = codepoint;
    let mut position = length - 1;
    while position > 0 {
        written[position] = ((rest | 0x80) & 0xBF) as u8;
        rest >>= 6;
        position -= 1;
    }
    written[0] = if length > 1 {
        ((rest | first_byte_mark) & 0xFF) as u8
    } else {
        (rest & 0x7F) as u8
    };

    Ok(1 + length)
}

/// The original's `parse_hex4`: each digit is added, then the sum is shifted to make place for the next one;
/// a byte that is not a hexadecimal digit makes the whole number 0.
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
