//! The leaf `print_string`: a text written as a JSON string, with its quotes, as cJSON's `print_string_ptr`.

const HEX: &[u8; 16] = b"0123456789abcdef";

fn put(out: &mut [u8], at: &mut usize, byte: u8) -> Result<(), ()> {
    *out.get_mut(*at).ok_or(())? = byte;
    *at += 1;
    Ok(())
}

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    // The original takes a text ended by a zero byte: one inside the text is refused.
    if input.contains(&0) {
        return Err(());
    }
    let mut at = 0usize;
    put(out, &mut at, b'"')?;
    for &byte in input {
        let short = match byte {
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            0x08 => Some(b'b'),
            0x0c => Some(b'f'),
            b'\n' => Some(b'n'),
            b'\r' => Some(b'r'),
            b'\t' => Some(b't'),
            _ => None,
        };
        match short {
            Some(letter) => {
                put(out, &mut at, b'\\')?;
                put(out, &mut at, letter)?;
            }
            None if byte < 32 => {
                // The other control bytes: \u00XX, in lowercase hexadecimal.
                put(out, &mut at, b'\\')?;
                put(out, &mut at, b'u')?;
                put(out, &mut at, b'0')?;
                put(out, &mut at, b'0')?;
                put(out, &mut at, HEX[usize::from(byte >> 4)])?;
                put(out, &mut at, HEX[usize::from(byte & 15)])?;
            }
            // Every other byte is copied as it is, bytes above 127 too.
            None => put(out, &mut at, byte)?,
        }
    }
    put(out, &mut at, b'"')?;
    Ok(at)
}
