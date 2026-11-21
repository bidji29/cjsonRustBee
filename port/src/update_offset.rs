//! The leaf `update_offset`: cJSON's `update_offset`, over a print buffer given as its offset (two bytes, low
//! byte first) then its content. The original moves the offset by the length of the C string that starts there,
//! that is to the first zero byte at or after the offset. Refused when no zero byte lies at or after the offset
//! (the original would read past the buffer's end).

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 2 || out.len() < 2 {
        return Err(());
    }
    let offset = (input[0] as usize) | ((input[1] as usize) << 8);
    let content = &input[2..];
    if offset >= content.len() {
        return Err(());
    }
    let mut new_offset = offset;
    loop {
        if new_offset >= content.len() {
            return Err(());
        }
        if content[new_offset] == 0 {
            break;
        }
        new_offset += 1;
    }
    if new_offset > 0xFFFF {
        return Err(());
    }
    out[0] = (new_offset & 0xFF) as u8;
    out[1] = (new_offset >> 8) as u8;
    Ok(2)
}
