//! The leaf `parse_string`: a JSON string's text read into its value, as cJSON's `parse_string`.
//!
//! Input: the room (two bytes, low byte first), then a buffer: its offset (two bytes, low byte first), then its
//! content. Answer: one byte, 1 on success and 0 on failure; the offset the original leaves (two bytes, low byte
//! first); then, on success, the block the original asked for, as it leaves it.
//!
//! As in the original, a first pass finds the closing quote, pairing each backslash with the byte after it, and
//! sizes the block from that; a second pass then decodes the escapes. On a failure before the second pass the
//! offset is left one past the opening byte; on a failure in it, at the backslash of the escape that failed.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 4 {
        return Err(());
    }
    let room = usize::from(input[0]) | (usize::from(input[1]) << 8);
    let offset = usize::from(input[2]) | (usize::from(input[3]) << 8);
    let content = &input[4..];
    let length = content.len();
    if offset >= length {
        // No byte at the offset: the original reads it without a check.
        return Err(());
    }
    if out.len() < 3 {
        return Err(());
    }

    let (parsed, left, asked) = parse(content, offset, room, &mut out[3..]);
    out[0] = u8::from(parsed);
    out[1] = (left & 0xFF) as u8;
    out[2] = ((left >> 8) & 0xFF) as u8;
    if parsed {
        Ok(3 + asked)
    } else {
        Ok(3)
    }
}

/// The original's work: whether it succeeds, the offset it leaves, and the size of the block it asked for. The
/// decoded bytes are written at the start of `block`, which is zeroed (so the terminating zero is there already).
fn parse(content: &[u8], offset: usize, room: usize, block: &mut [u8]) -> (bool, usize, usize) {
    let length = content.len();
    let start = offset + 1;

    if content[offset] != b'"' {
        return (false, start, 0);
    }

    // The first pass: where the string ends, and how many escapes it holds.
    let mut end = start;
    let mut skipped = 0usize;
    while end < length && content[end] != b'"' {
        if content[end] == b'\\' {
            if end + 1 >= length {
                // The last byte of the content is a backslash.
                return (false, start, 0);
            }
            skipped += 1;
            end += 1;
        }
        end += 1;
    }
    if end >= length || content[end] != b'"' {
        return (false, start, 0);
    }

    // At most this much is needed, and one byte more for the terminating zero.
    let asked = (end - offset) - skipped + 1;
    if asked > room {
        // The allocation fails.
        return (false, start, 0);
    }

    // The second pass: the bytes, escapes decoded.
    let mut here = start;
    let mut written = 0usize;
    while here < end {
        if content[here] != b'\\' {
            block[written] = content[here];
            written += 1;
            here += 1;
            continue;
        }
        let mut sequence_length = 2usize;
        match content[here + 1] {
            b'b' => {
                block[written] = 0x08;
                written += 1;
            }
            b'f' => {
                block[written] = 0x0C;
                written += 1;
            }
            b'n' => {
                block[written] = b'\n';
                written += 1;
            }
            b'r' => {
                block[written] = b'\r';
                written += 1;
            }
            b't' => {
                block[written] = b'\t';
                written += 1;
            }
            b'"' | b'\\' | b'/' => {
                block[written] = content[here + 1];
                written += 1;
            }
            b'u' => match utf16_literal(&content[here..end], &mut block[written..]) {
                Some((read, count)) => {
                    sequence_length = read;
                    written += count;
                }
                None => return (false, here, 0),
            },
            _ => return (false, here, 0),
        }
        here += sequence_length;
    }

    // The terminating zero: the block is zeroed already.
    (true, end + 1, asked)
}

/// The original's `utf16_literal_to_utf8`, given the text from the backslash to the closing quote (excluded):
/// the count of bytes it read (6 or 12) and the count of UTF-8 bytes it wrote; `None` when it fails.
fn utf16_literal(text: &[u8], output: &mut [u8]) -> Option<(usize, usize)> {
    if text.len() < 6 {
        return None;
    }
    let first_code = hex4(&text[2..6]);

    if (0xDC00..=0xDFFF).contains(&first_code) {
        return None;
    }

    let (read, codepoint): (usize, u32) = if (0xD800..=0xDBFF).contains(&first_code) {
        if text.len() - 6 < 6 {
            return None;
        }
        if text[6] != b'\\' || text[7] != b'u' {
            return None;
        }
        let second_code = hex4(&text[8..12]);
        if !(0xDC00..=0xDFFF).contains(&second_code) {
            return None;
        }
        (12, 0x10000 + (((first_code & 0x3FF) << 10) | (second_code & 0x3FF)))
    } else {
        (6, first_code)
    };

    let (count, first_byte_mark): (usize, u32) = if codepoint < 0x80 {
        (1, 0)
    } else if codepoint < 0x800 {
        (2, 0xC0)
    } else if codepoint < 0x10000 {
        (3, 0xE0)
    } else if codepoint <= 0x10FFFF {
        (4, 0xF0)
    } else {
        return None;
    };

    // The continuation bytes, last first: 10xxxxxx.
    let mut rest = codepoint;
    let mut position = count - 1;
    while position > 0 {
        output[position] = ((rest | 0x80) & 0xBF) as u8;
        rest >>= 6;
        position -= 1;
    }
    output[0] = if count > 1 {
        ((rest | first_byte_mark) & 0xFF) as u8
    } else {
        (rest & 0x7F) as u8
    };

    Some((read, count))
}

/// The original's `parse_hex4`: a byte that is not a hexadecimal digit makes the whole number 0.
fn hex4(digits: &[u8]) -> u32 {
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
