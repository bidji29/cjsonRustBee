//! The leaf `duplicate`: cJSON's `cJSON_Duplicate`, with `cJSON_Duplicate_rec`, `cJSON_New_Item` and
//! `cJSON_strdup`. The input is the room (two bytes, low byte first, counted in allocations), the byte `recurse`,
//! then one named item in the tree's byte form. The answer is the copy in the same form, or a refusal when the
//! original returns NULL (an allocation fails, or the depth limit is reached) or when the form is not a whole item.
//!
//! The original asks for one block for each item it copies (`cJSON_New_Item`), then one for its `valuestring` when
//! it has one, then one for its name when it has one (`cJSON_strdup`, an empty text too); with `recurse`, it then
//! copies each child whole, in order. Without `recurse` the copy has no children. The walk below reads the item as
//! the driver reads it, asks for each block where the original does, and writes each copied item as it goes.

const NUMBER: u8 = 8;
const STRING: u8 = 16;
const ARRAY: u8 = 32;
const OBJECT: u8 = 64;
const RAW: u8 = 128;
/// `CJSON_CIRCULAR_LIMIT`: an item at this depth that has a child is not copied.
const CIRCULAR_LIMIT: usize = 10000;
/// The length that stands for a `NULL` text.
const NULL_TEXT: usize = 0xFFFF;

struct Walk<'a> {
    input: &'a [u8],
    at: usize,
    out: &'a mut [u8],
    written: usize,
    room: usize,
    calls: usize,
}

impl<'a> Walk<'a> {
    /// The driver's allocator: each of the first `room` calls gives a block, the next fails.
    fn allocate(&mut self) -> Result<(), ()> {
        if self.calls >= self.room {
            return Err(());
        }
        self.calls += 1;
        Ok(())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ()> {
        let input: &'a [u8] = self.input;
        if input.len() - self.at < count {
            return Err(());
        }
        let bytes = &input[self.at..self.at + count];
        self.at += count;
        Ok(bytes)
    }

    fn take_u16(&mut self) -> Result<usize, ()> {
        let bytes = self.take(2)?;
        Ok((bytes[0] as usize) | ((bytes[1] as usize) << 8))
    }

    fn put(&mut self, bytes: &[u8]) -> Result<(), ()> {
        if self.out.len() - self.written < bytes.len() {
            return Err(());
        }
        self.out[self.written..self.written + bytes.len()].copy_from_slice(bytes);
        self.written += bytes.len();
        Ok(())
    }

    fn put_u16(&mut self, value: usize) -> Result<(), ()> {
        self.put(&[(value & 255) as u8, ((value >> 8) & 255) as u8])
    }

    /// A text of the form: `None` for `NULL`; refused when cut short or when it holds a zero byte.
    fn take_text(&mut self) -> Result<Option<&'a [u8]>, ()> {
        let length = self.take_u16()?;
        if length == NULL_TEXT {
            return Ok(None);
        }
        let text = self.take(length)?;
        if text.contains(&0) {
            return Err(());
        }
        Ok(Some(text))
    }

    fn put_text(&mut self, text: Option<&[u8]>) -> Result<(), ()> {
        match text {
            None => self.put_u16(NULL_TEXT),
            Some(bytes) => {
                self.put_u16(bytes.len())?;
                self.put(bytes)
            }
        }
    }

    /// One item and all it holds. With `copy`, the item is duplicated as `cJSON_Duplicate_rec` does it, at the
    /// depth given, its children too when `recurse` is set; without it, the item is only read (it belongs to an
    /// item copied without its children, or lies below one).
    fn item(&mut self, named: bool, copy: bool, recurse: bool, depth: usize) -> Result<(), ()> {
        let kind = self.take(1)?[0];
        let name = if named { self.take_text()? } else { None };
        let mut number: &[u8] = &[];
        let mut text: Option<&[u8]> = None;
        let mut count = 0usize;
        match kind {
            NUMBER => number = self.take(12)?,
            STRING | RAW => text = self.take_text()?,
            ARRAY | OBJECT => count = self.take_u16()?,
            _ => {}
        }
        if copy {
            // cJSON_New_Item, then the valuestring, then the name.
            self.allocate()?;
            if text.is_some() {
                self.allocate()?;
            }
            if name.is_some() {
                self.allocate()?;
            }
            self.put(&[kind])?;
            if named {
                self.put_text(name)?;
            }
            match kind {
                NUMBER => self.put(number)?,
                STRING | RAW => self.put_text(text)?,
                ARRAY | OBJECT => self.put_u16(if recurse { count } else { 0 })?,
                _ => {}
            }
        }
        let children = copy && recurse;
        let mut index = 0usize;
        while index < count {
            if children && depth >= CIRCULAR_LIMIT {
                return Err(());
            }
            self.item(kind == OBJECT, children, true, depth + 1)?;
            index += 1;
        }
        Ok(())
    }
}

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 4 {
        return Err(());
    }
    let room = (input[0] as usize) | ((input[1] as usize) << 8);
    let recurse = input[2] != 0;
    let mut walk = Walk { input, at: 3, out, written: 0, room, calls: 0 };
    walk.item(true, true, recurse, 0)?;
    if walk.at != input.len() {
        return Err(());
    }
    Ok(walk.written)
}
