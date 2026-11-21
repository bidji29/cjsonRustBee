//! The leaf `buffer_skip_whitespace`: cJSON's `buffer_skip_whitespace`, over a buffer given as its offset (two
//! bytes, low byte first) then its content. When the offset cannot be read (at the end of the content or beyond),
//! it is left as it is. Otherwise every byte up to 32 is skipped, and if that reaches the end of the content, the
//! offset steps back by one, onto the last byte.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 2 || out.len() < 2 {
        return Err(());
    }
    let mut offset = (input[0] as usize) | ((input[1] as usize) << 8);
    let content = &input[2..];
    let length = content.len();
    // cannot_access_at_index(buffer, 0): offset >= length, and the buffer is returned untouched.
    if offset < length {
        while offset < length && content[offset] <= 32 {
            offset += 1;
        }
        // length > 0 here, so this never goes below zero.
        if offset == length {
            offset -= 1;
        }
    }
    if offset > 0xFFFF {
        return Err(());
    }
    out[0] = (offset & 0xFF) as u8;
    out[1] = (offset >> 8) as u8;
    Ok(2)
}
