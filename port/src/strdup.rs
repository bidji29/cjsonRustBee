//! The leaf `strdup`: cJSON's `cJSON_strdup`. The input is the room (two bytes, low byte first), then a text
//! without a zero byte. The original asks its allocator for the text's length plus one (its zero), fails when
//! the allocator fails (the size is larger than the room), and copies the text and its zero into the block.

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 2 {
        return Err(());
    }
    let room = (input[0] as usize) | ((input[1] as usize) << 8);
    let text = &input[2..];
    if text.contains(&0) {
        return Err(());
    }
    let asked = text.len() + 1;
    if asked > room || asked > out.len() {
        return Err(());
    }
    out[..text.len()].copy_from_slice(text);
    out[text.len()] = 0;
    Ok(asked)
}
