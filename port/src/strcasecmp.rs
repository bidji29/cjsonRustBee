//! The leaf `strcasecmp`: two names compared without regard to case, as cJSON's `case_insensitive_strcmp`.
//!
//! Given two texts, each ended by one zero byte (`first 0 second 0`), it answers four bytes: the number the
//! original returns, low byte first, as a 32-bit two's complement. Anything else is refused.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if out.len() < 4 {
        return Err(());
    }
    let (first, second) = split(input)?;
    let value = compare(first, second);
    out[..4].copy_from_slice(&value.to_le_bytes());
    Ok(4)
}

/// The two texts, without their ending zero bytes. The input holds exactly two zero bytes: one that ends the
/// first text and is not the last byte, and the last byte, which ends the second text.
fn split(input: &[u8]) -> Result<(&[u8], &[u8]), ()> {
    let (&last, body) = input.split_last().ok_or(())?;
    if last != 0 {
        return Err(());
    }
    let zero = body.iter().position(|&byte| byte == 0).ok_or(())?;
    let first = &body[..zero];
    let second = &body[zero + 1..];
    if second.contains(&0) {
        return Err(());
    }
    Ok((first, second))
}

/// The original's loop: bytes are compared in lower case until two differ or the first text ends; the answer is
/// then 0, or the difference of the two bytes that differ, each in lower case. The end of a text counts as a
/// zero byte, as in C.
fn compare(first: &[u8], second: &[u8]) -> i32 {
    let mut at = 0;
    loop {
        let a = lower(first.get(at).copied().unwrap_or(0));
        let b = lower(second.get(at).copied().unwrap_or(0));
        if a != b {
            return i32::from(a) - i32::from(b);
        }
        if at >= first.len() {
            return 0;
        }
        at += 1;
    }
}

/// C's `tolower` in the "C" locale, on an unsigned byte: only `A` to `Z` change.
fn lower(byte: u8) -> u8 {
    match byte {
        b'A'..=b'Z' => byte + (b'a' - b'A'),
        _ => byte,
    }
}
