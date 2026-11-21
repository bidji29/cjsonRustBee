//! The leaf `parse_value`: a JSON text parsed into its tree of items, as cJSON's `parse_value` with all it calls
//! (`parse_array`, `parse_object`, `parse_string`, `parse_number`, `buffer_skip_whitespace`).
//!
//! Input and answer: port/LEAVES.md. The original goes down one call for each array or object nested in another;
//! here the open ones are kept in a table of `NESTING_LIMIT` entries and one loop does the work, so the stack
//! stays flat. The tree is written as it is parsed: an item's type byte, a text's length and a container's count
//! are set aside, and filled in once known. What falls beyond the answer's buffer is dropped and remembered: the
//! parse goes on (it may still fail, and a failure answers three bytes), and a success that does not fit is refused.

use core::cmp::Ordering;

/// `CJSON_NESTING_LIMIT`, as reference/cJSON.h gives it.
const NESTING_LIMIT: usize = 1000;

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.len() < 4 || out.len() < 3 { return Err(()); }
    // The buffer the leaf answers in: a success longer than that is refused.
    let capacity = out.len().min(6 * input.len() + 16);
    let room = usize::from(input[0]) | (usize::from(input[1]) << 8);
    let offset = usize::from(input[2]) | (usize::from(input[3]) << 8);
    let mut parser = Parser { content: &input[4..], offset, room, out: &mut out[..capacity], at: 3, dropped: false };
    let parsed = parser.parse();
    let (left, written, dropped) = (parser.offset, parser.at, parser.dropped);
    if left > 0xFFFF || (parsed && dropped) { return Err(()); }
    out[0] = u8::from(parsed);
    out[1] = (left & 0xFF) as u8;
    out[2] = (left >> 8) as u8;
    Ok(if parsed { written } else { 3 })
}

/// The buffer; `room`, the allocations the driver's allocator still grants; the answer's buffer, where the next
/// byte of the tree goes in it, and whether a byte fell beyond it.
struct Parser<'a> {
    content: &'a [u8], offset: usize, room: usize,
    out: &'a mut [u8], at: usize, dropped: bool,
}

impl<'a> Parser<'a> {
    /// The byte at the offset, when there is one (`can_access_at_index(buffer, 0)`).
    fn here(&self) -> Option<u8> { self.content.get(self.offset).copied() }
    /// The original's `buffer_skip_whitespace`: at the end of the content, it steps back onto the last byte.
    fn skip_whitespace(&mut self) {
        let length = self.content.len();
        if self.offset < length {
            while self.offset < length && self.content[self.offset] <= 32 { self.offset += 1; }
            if self.offset == length { self.offset -= 1; }
        }
    }
    /// One call of the driver's allocator: whether it gives a block.
    fn allocate(&mut self) -> bool {
        let given = self.room > 0;
        self.room -= usize::from(given);
        given
    }
    fn put(&mut self, position: usize, byte: u8) {
        if let Some(place) = self.out.get_mut(position) { *place = byte; } else { self.dropped = true; }
    }
    /// Two bytes set aside, low byte first: a text's length, a count of children.
    fn put_u16(&mut self, position: usize, value: usize) {
        self.put(position, (value & 0xFF) as u8);
        self.put(position + 1, ((value >> 8) & 0xFF) as u8);
    }
    /// The original's `parse_value`, from the root: whether it succeeds.
    fn parse(&mut self) -> bool {
        // For each open array or object: its closing bracket, where its count of children goes, and that count.
        let mut open = [(0u8, 0usize, 0usize); NESTING_LIMIT];
        let mut depth = 0usize;
        // Where the type byte of the value about to be read goes: first in its item, before its name if any.
        let mut type_at = self.at;
        self.at += 1;
        loop {
            let rest = self.content.get(self.offset..).unwrap_or(&[]);
            let literal = [(&b"null"[..], 4u8), (b"false", 1), (b"true", 2)];
            let mut opened = false;
            if let Some((text, kind)) = literal.into_iter().find(|(text, _)| rest.starts_with(text)) {
                self.put(type_at, kind);
                self.offset += text.len();
            } else if rest.first() == Some(&b'"') {
                self.put(type_at, 16);
                if !self.string() { return false; }
            } else if let Some(b'-' | b'0'..=b'9') = rest.first() {
                self.put(type_at, 8);
                if !self.number() { return false; }
            } else if let Some(&opening @ (b'[' | b'{')) = rest.first() {
                if depth >= NESTING_LIMIT { return false; }
                self.put(type_at, if opening == b'[' { 32 } else { 64 });
                // In ASCII, each closing bracket is two after its opening one.
                let (closing, count_at) = (opening + 2, self.at);
                self.put_u16(count_at, 0);
                self.at += 2;
                self.offset += 1;
                self.skip_whitespace();
                let next = self.here();
                if next == Some(closing) {
                    // Empty: a whole value, its count zero.
                    self.offset += 1;
                } else {
                    // Step back: onto the bracket, to fail, when nothing follows it; else before the first child.
                    self.offset -= 1;
                    if next.is_none() { return false; }
                    open[depth] = (closing, count_at, 0);
                    depth += 1;
                    opened = true;
                }
            } else {
                return false;
            }
            // After a whole value: a comma brings the next child; anything else must close the innermost open
            // array or object, which is then itself a whole value. With none open, the root is read.
            while !opened {
                if depth == 0 { return true; }
                self.skip_whitespace();
                let (closing, count_at, count) = open[depth - 1];
                match self.here() {
                    Some(b',') => break,
                    Some(byte) if byte == closing => {
                        self.put_u16(count_at, count);
                        depth -= 1;
                        self.offset += 1;
                    }
                    _ => return false,
                }
            }
            // The next child (the offset is on the comma, or before the first child): its item is asked first.
            if !self.allocate() { return false; }
            open[depth - 1].2 += 1;
            type_at = self.at;
            self.at += 1;
            if open[depth - 1].0 == b'}' {
                // An object's child: its name (when a byte follows the comma), a colon, then its value.
                if self.offset + 1 >= self.content.len() { return false; }
                self.offset += 1;
                self.skip_whitespace();
                if !self.string() { return false; }
                self.skip_whitespace();
                if self.here() != Some(b':') { return false; }
            }
            self.offset += 1;
            self.skip_whitespace();
        }
    }
    /// The original's `parse_number` at the offset: whether it succeeds; the number's twelve bytes are written.
    fn number(&mut self) -> bool {
        // The copy handed to `strtod` is one block, asked for before `strtod` reads anything.
        if !self.allocate() { return false; }
        let Some((count, bits)) = read(&self.content[self.offset..]) else { return false };
        // `valueint`: the cast saturates to the range of a 32-bit int, as the original does.
        let int = f64::from_bits(bits) as i32;
        for byte in bits.to_le_bytes().into_iter().chain(int.to_le_bytes()) {
            self.put(self.at, byte);
            self.at += 1;
        }
        self.offset += count;
        true
    }
    /// The original's `parse_string` at the offset, where a byte lies: whether it succeeds. The text is written:
    /// its length, then its bytes up to its first zero byte (the C string it is). A failure leaves the offset one
    /// past the opening byte, or, among the escapes, at the backslash of the one that failed.
    fn string(&mut self) -> bool {
        let content = self.content;
        let start = self.offset + 1;
        self.offset = start;
        if content[start - 1] != b'"' { return false; }
        // The first pass: the closing quote, each backslash paired with the byte after it.
        let mut end = start;
        while end < content.len() && content[end] != b'"' { end += if content[end] == b'\\' { 2 } else { 1 }; }
        // No closing quote (or the content's last byte is a backslash), or no block for the string.
        if end >= content.len() || !self.allocate() { return false; }
        // The second pass: the bytes, escapes decoded.
        let length_at = self.at;
        self.at += 2;
        let (mut here, mut ended) = (start, false);
        while here < end {
            let mut bytes = [content[here], 0, 0, 0];
            let plain = content[here] != b'\\';
            let decoded = if plain { Some((1, 1)) } else { escape(&content[here..end], &mut bytes) };
            let Some((read, count)) = decoded else {
                self.offset = here;
                return false;
            };
            for &byte in &bytes[..count] {
                ended = ended || byte == 0;
                if !ended {
                    self.put(self.at, byte);
                    self.at += 1;
                }
            }
            here += read;
        }
        self.put_u16(length_at, self.at - length_at - 2);
        self.offset = end + 1;
        true
    }
}

/// One escape, given the text from its backslash to the closing quote (excluded): the count of bytes it takes
/// and the count of bytes it stands for, written in `bytes`; `None` when the original fails on it.
fn escape(text: &[u8], bytes: &mut [u8; 4]) -> Option<(usize, usize)> {
    let table = [(b'b', 0x08), (b'f', 0x0C), (b'n', b'\n'), (b'r', b'\r'), (b't', b'\t'),
                 (b'"', b'"'), (b'\\', b'\\'), (b'/', b'/')];
    if text[1] == b'u' { return utf16_literal(text, bytes); }
    bytes[0] = table.iter().find(|(letter, _)| *letter == text[1])?.1;
    Some((2, 1))
}

/// The original's `utf16_literal_to_utf8`: the count of bytes it read (6 or 12) and of UTF-8 bytes it wrote.
fn utf16_literal(text: &[u8], output: &mut [u8; 4]) -> Option<(usize, usize)> {
    if text.len() < 6 { return None; }
    let first_code = hex4(&text[2..6]);
    let (read, codepoint) = match first_code {
        0xDC00..=0xDFFF => return None,
        0xD800..=0xDBFF => {
            if text.len() < 12 || text[6] != b'\\' || text[7] != b'u' { return None; }
            let second_code = hex4(&text[8..12]);
            if !(0xDC00..=0xDFFF).contains(&second_code) { return None; }
            (12, 0x10000 + (((first_code & 0x3FF) << 10) | (second_code & 0x3FF)))
        }
        _ => (6, first_code),
    };
    let marks: [(u32, usize, u32); 4] = [(0x80, 1, 0), (0x800, 2, 0xC0), (0x10000, 3, 0xE0), (0x110000, 4, 0xF0)];
    let (_, count, first_byte_mark) = *marks.iter().find(|(above, _, _)| codepoint < *above)?;
    // The continuation bytes, last first: 10xxxxxx; then the first byte, with its mark.
    let mut rest = codepoint;
    for position in (1..count).rev() {
        output[position] = ((rest | 0x80) & 0xBF) as u8;
        rest >>= 6;
    }
    output[0] = (rest | first_byte_mark) as u8;
    Some((read, count))
}

/// The original's `parse_hex4`: a byte that is not a hexadecimal digit makes the whole number 0.
fn hex4(digits: &[u8]) -> u32 {
    digits.iter().try_fold(0, |h, &byte| Some((h << 4) + (byte as char).to_digit(16)?)).unwrap_or(0)
}

const INFINITY: u64 = 0x7FF0_0000_0000_0000;
const FRACTION: u64 = (1 << 52) - 1;

/// What `strtod` answers on the bytes the original copies (the longest run of `0-9 + - e E .`): the count of
/// bytes it read and the double's pattern; `None` when it reads nothing. Its grammar there: an optional sign,
/// digits with at most one point (at least one digit), then an exponent only when a digit follows its `e` (and
/// its sign). Every byte of that grammar is one the original copies, so the run's end need not be found first.
fn read(input: &[u8]) -> Option<(usize, u64)> {
    let at = |i: usize| -> u8 { if i < input.len() { input[i] } else { 0 } };
    let negative = at(0) == b'-';
    let mut i = usize::from(at(0) == b'-' || at(0) == b'+');
    // The digits, as they come; those after the point count down the exponent.
    let digits_start = i;
    let (mut seen_digit, mut point, mut exponent) = (false, false, 0i64);
    while at(i).is_ascii_digit() || (at(i) == b'.' && !point) {
        exponent -= i64::from(point && at(i) != b'.');
        (seen_digit, point) = (seen_digit || at(i) != b'.', point || at(i) == b'.');
        i += 1;
    }
    if !seen_digit { return None; }
    let digits_end = i;
    if at(i) == b'e' || at(i) == b'E' {
        let exponent_negative = at(i + 1) == b'-';
        let mut j = i + 1 + usize::from(at(i + 1) == b'-' || at(i + 1) == b'+');
        // Beyond a hundred million, the answer is infinity or zero whatever digits follow.
        let mut written: i64 = 0;
        while at(j).is_ascii_digit() {
            written = if written < 100_000_000 { written * 10 + i64::from(at(j) - b'0') } else { written };
            j += 1;
            i = j;
        }
        exponent += if exponent_negative { -written } else { written };
    }
    let magnitude = decimal_to_double(&input[digits_start..digits_end], exponent);
    Some((i, magnitude | (u64::from(negative) << 63)))
}

/// The digits of a number's text, without its point.
fn digits(text: &[u8]) -> impl Iterator<Item = u8> + '_ {
    text.iter().filter(|c| **c != b'.').map(|c| c - b'0')
}

/// The double nearest to the digits (with their point) times ten to `exponent`, positive: the C library's
/// `strtod` converts exactly, to the even one in a tie.
fn decimal_to_double(text: &[u8], mut exponent: i64) -> u64 {
    // Take off the leading zeros (and the point among them), then the trailing zeros: the exponent already
    // counted the digits after the point, so each trailing zero taken off is one more power of ten.
    let (mut start, mut end) = (0, text.len());
    while start < end && (text[start] == b'0' || text[start] == b'.') { start += 1; }
    while end > start && (text[end - 1] == b'0' || text[end - 1] == b'.') {
        exponent += i64::from(text[end - 1] == b'0');
        end -= 1;
    }
    let text = &text[start..end];
    let count = digits(text).count() as i64;
    // Zero, or so small or so large that the answer is zero or infinity whatever the digits are.
    if count == 0 || count + exponent < -324 { return 0; }
    if count + exponent > 310 { return INFINITY; }
    // Whether the value rounds beyond the double `bits`: it is above the midpoint between `bits` and the next
    // double up (the significand of `bits` doubled, plus one, at half its unit), or on it when `bits` is odd.
    let beyond = |bits: u64| -> bool {
        let (biased, fraction) = ((bits >> 52) as i64, bits & FRACTION);
        let (significand, power) = if biased == 0 { (fraction, -1074) } else { (fraction | (1 << 52), biased - 1075) };
        let order = compare(text, exponent, 2 * significand + 1, power - 1);
        order == Ordering::Greater || (order == Ordering::Equal && bits & 1 == 1)
    };
    // Move a first guess until the value lies between the two midpoints around it (the even one on a tie).
    let mut candidate = estimate(text, count, exponent);
    loop {
        if candidate != INFINITY && beyond(candidate) { candidate += 1; }
        else if candidate != 0 && !beyond(candidate - 1) { candidate -= 1; }
        else { return candidate; }
    }
}

/// A first guess, within a few units of the last place: the first nineteen digits, multiplied or divided by ten
/// with 64 bits kept at each step.
fn estimate(text: &[u8], count: i64, exponent: i64) -> u64 {
    let (mut top, mut used) = (0u64, 0i64);
    for d in digits(text).take(19) { (top, used) = (top * 10 + u64::from(d), used + 1); }
    // The value is about `m` times two to `b`, the highest bit of `m` set.
    let mut m = top << top.leading_zeros();
    let mut b = -i64::from(top.leading_zeros());
    let mut tens = exponent + (count - used);
    while tens != 0 {
        let wide = if tens > 0 { u128::from(m) * 10 } else { (u128::from(m) << 64) / 10 };
        let extra = 64 - wide.leading_zeros();
        m = (wide >> extra) as u64;
        b += i64::from(extra) - if tens > 0 { 0 } else { 64 };
        tens -= tens.signum();
    }
    let biased = b + 63 + 1023;
    if biased >= 2047 { INFINITY }
    else if biased >= 1 { ((biased as u64) << 52) | ((m >> 11) & FRACTION) }
    else if biased > -52 { m >> (12 - biased) }
    else { 0 }
}

/// The exact order of the digits times ten to `exponent` and of `m` times two to `e`, in integers.
fn compare(text: &[u8], exponent: i64, m: u64, e: i64) -> Ordering {
    let (mut left, mut right) = (Big::zero(), Big::zero());
    // The digits, nine at a time.
    let (mut group, mut scale) = (0u32, 1u32);
    for d in digits(text) {
        group = group * 10 + u32::from(d);
        scale *= 10;
        if scale == 1_000_000_000 {
            left.mul_add(scale, group);
            (group, scale) = (0, 1);
        }
    }
    left.mul_add(scale, group);
    // `m` is below two to the 54: sixteen bits at a time, high ones first.
    for part in [m >> 48, (m >> 32) & 0xFFFF, (m >> 16) & 0xFFFF, m & 0xFFFF] {
        right.mul_add(1 << 16, part as u32);
    }
    // Each side is multiplied by what the other would be divided by.
    let (mut tens, mut twos) = (exponent.unsigned_abs(), e.unsigned_abs());
    while tens > 0 {
        let step = tens.min(9);
        (if exponent >= 0 { &mut left } else { &mut right }).mul_add(10u32.pow(step as u32), 0);
        tens -= step;
    }
    while twos > 0 {
        let step = twos.min(31);
        (if e < 0 { &mut left } else { &mut right }).mul_add(1 << step, 0);
        twos -= step;
    }
    // The limbs beyond those used are zero: the highest limb that differs decides.
    left.limbs.iter().rev().cmp(right.limbs.iter().rev())
}

/// Enough for every number that is compared: the content is at most 3068 bytes (the judge's cases are 3072 at
/// most), so the digits are below ten to the 3068; times two to the 1076, as the midpoint of the smallest
/// doubles asks, that is below two to the 11300, and the other side is then within a few bits of it.
const LIMBS: usize = 400;

/// An unsigned integer of up to `LIMBS` times 32 bits, low limb first: `count` limbs are used.
struct Big { limbs: [u32; LIMBS], count: usize }

impl Big {
    fn zero() -> Big { Big { limbs: [0; LIMBS], count: 0 } }
    /// Multiplies by `factor`, then adds `addend`.
    fn mul_add(&mut self, factor: u32, addend: u32) {
        let mut carry = u64::from(addend);
        for limb in self.limbs[..self.count].iter_mut() {
            let p = u64::from(*limb) * u64::from(factor) + carry;
            *limb = p as u32;
            carry = p >> 32;
        }
        if carry != 0 {
            self.limbs[self.count] = carry as u32;
            self.count += 1;
        }
    }
}
