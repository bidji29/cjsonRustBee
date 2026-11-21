//! The leaf `skip_utf8_bom`: cJSON's `skip_utf8_bom`, over a buffer given as its offset (two bytes, low byte
//! first) then its content. The original fails unless the offset is 0; it then skips a UTF-8 byte order mark
//! (EF BB BF) only when it can access index 4, that is when the content holds more than four bytes.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 2 || out.len() < 2 {
        return Err(());
    }
    let offset = (input[0] as usize) | ((input[1] as usize) << 8);
    let content = &input[2..];
    if offset != 0 {
        return Err(());
    }
    let mut new_offset = offset;
    // can_access_at_index(buffer, 4): offset + 4 < length. strncmp over three bytes equals the mark exactly
    // when the first three bytes are EF BB BF (the mark holds no zero byte).
    if 4 < content.len() && content[0] == 0xEF && content[1] == 0xBB && content[2] == 0xBF {
        new_offset += 3;
    }
    out[0] = (new_offset & 0xFF) as u8;
    out[1] = (new_offset >> 8) as u8;
    Ok(2)
}
