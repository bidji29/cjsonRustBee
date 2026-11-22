//! The leaf `compare`: whether two trees of items count as equal, as cJSON's `cJSON_Compare`, with all it calls
//! (`compare_double`, `get_object_item`, `case_insensitive_strcmp`).
//!
//! Given one byte, `case_sensitive`, then the item `a` in the tree's byte form, then the item `b` in the same
//! form or nothing (then `b` is `a` itself), it answers one byte: 1 when the original returns true, else 0.
//! Refused: an item cut short, a text that holds a zero byte, bytes after `b`.
//!
//! Nothing is built: the two trees are read where they lie in the input, an item being known by the place of
//! its type byte. The whole form is walked once first, so that every refusal is found whatever the comparison
//! would have read; after that walk, the comparison reads a form it knows to be whole.

const FALSE: u8 = 1;
const TRUE: u8 = 2;
const NULL: u8 = 4;
const NUMBER: u8 = 8;
const STRING: u8 = 16;
const ARRAY: u8 = 32;
const OBJECT: u8 = 64;
const RAW: u8 = 128;

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.is_empty() || out.is_empty() {
        return Err(());
    }
    let case_sensitive = input[0] != 0;
    let a = 1;
    let after_a = skip(input, a, false)?;
    let equal = if after_a == input.len() {
        // The same pointer twice: equal at once when the type is one the original knows, never otherwise.
        known(input[a])
    } else {
        let b = after_a;
        if skip(input, b, false)? != input.len() {
            return Err(());
        }
        compare(input, a, false, b, false, case_sensitive)?
    };
    out[0] = u8::from(equal);
    Ok(1)
}

/// Whether the type byte is one of the eight the original compares.
fn known(kind: u8) -> bool {
    matches!(kind, FALSE | TRUE | NULL | NUMBER | STRING | RAW | ARRAY | OBJECT)
}

/// Two bytes, low byte first.
fn count_at(input: &[u8], at: usize) -> Result<usize, ()> {
    let low = *input.get(at).ok_or(())?;
    let high = *input.get(at + 1).ok_or(())?;
    Ok(usize::from(low) | usize::from(high) << 8)
}

/// A text at `at`: `None` for NULL, else its bytes; and the place after it. Refused when it is cut short or
/// holds a zero byte.
fn text_at(input: &[u8], at: usize) -> Result<(Option<&[u8]>, usize), ()> {
    let length = count_at(input, at)?;
    let start = at + 2;
    if length == 0xFFFF {
        return Ok((None, start));
    }
    let bytes = input.get(start..start + length).ok_or(())?;
    if bytes.contains(&0) {
        return Err(());
    }
    Ok((Some(bytes), start + length))
}

/// The place of what an item's type holds: after its type byte, and after its name when it has one.
fn body_of(input: &[u8], at: usize, named: bool) -> Result<usize, ()> {
    if at >= input.len() {
        return Err(());
    }
    if named {
        Ok(text_at(input, at + 1)?.1)
    } else {
        Ok(at + 1)
    }
}

/// The place after the item at `at`, all it holds passed over. Refused when the form is cut short or a text in
/// it holds a zero byte.
fn skip(input: &[u8], at: usize, named: bool) -> Result<usize, ()> {
    let kind = *input.get(at).ok_or(())?;
    let body = body_of(input, at, named)?;
    match kind {
        NUMBER => {
            if input.len() - body < 12 {
                return Err(());
            }
            Ok(body + 12)
        }
        STRING | RAW => Ok(text_at(input, body)?.1),
        ARRAY | OBJECT => {
            let count = count_at(input, body)?;
            let mut child = body + 2;
            let mut left = count;
            while left > 0 {
                child = skip(input, child, kind == OBJECT)?;
                left -= 1;
            }
            Ok(child)
        }
        _ => Ok(body),
    }
}

/// Eight bytes, low byte first, as the double whose pattern they are.
fn double_at(input: &[u8], at: usize) -> Result<f64, ()> {
    let bytes = input.get(at..at + 8).ok_or(())?;
    let mut pattern = [0u8; 8];
    pattern.copy_from_slice(bytes);
    Ok(f64::from_bits(u64::from_le_bytes(pattern)))
}

/// `fabs`: the same pattern with its sign bit cleared.
fn absolute(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

/// The original's `compare_double`.
fn doubles_equal(a: f64, b: f64) -> bool {
    let abs_a = absolute(a);
    let abs_b = absolute(b);
    let max_val = if abs_a > abs_b { abs_a } else { abs_b };
    absolute(a - b) <= max_val * f64::EPSILON
}

/// C's `tolower` in the "C" locale: only `A` to `Z` change.
fn lower(byte: u8) -> u8 {
    match byte {
        b'A'..=b'Z' => byte + (b'a' - b'A'),
        _ => byte,
    }
}

/// Whether `case_insensitive_strcmp` answers 0 for two texts that are not NULL. Neither holds a zero byte, so
/// they are equal only when they are as long as one another.
fn same_without_case(first: &[u8], second: &[u8]) -> bool {
    first.len() == second.len() && first.iter().zip(second.iter()).all(|(&x, &y)| lower(x) == lower(y))
}

/// The original's `get_object_item`: among `count` children that begin at `first`, the place of the first one
/// the name finds, if any.
fn find(
    input: &[u8],
    first: usize,
    count: usize,
    name: Option<&[u8]>,
    case_sensitive: bool,
) -> Result<Option<usize>, ()> {
    let name = match name {
        Some(name) => name,
        None => return Ok(None),
    };
    let mut child = first;
    let mut left = count;
    while left > 0 {
        let (child_name, _) = text_at(input, child + 1)?;
        match child_name {
            Some(child_name) => {
                let found = if case_sensitive { name == child_name } else { same_without_case(name, child_name) };
                if found {
                    return Ok(Some(child));
                }
            }
            None => {
                // With regard to case, the search stops at a child whose name is NULL; without, it passes over.
                if case_sensitive {
                    return Ok(None);
                }
            }
        }
        left -= 1;
        if left > 0 {
            child = skip(input, child, true)?;
        }
    }
    Ok(None)
}

/// Each child of the object `x` looked up by its name among the children of the object `y`, and compared with
/// the child found: one of the two loops of the original.
fn each_found(
    input: &[u8],
    x_first: usize,
    x_count: usize,
    y_first: usize,
    y_count: usize,
    case_sensitive: bool,
) -> Result<bool, ()> {
    let mut child = x_first;
    let mut left = x_count;
    while left > 0 {
        let (name, _) = text_at(input, child + 1)?;
        let other = match find(input, y_first, y_count, name, case_sensitive)? {
            Some(other) => other,
            None => return Ok(false),
        };
        if !compare(input, child, true, other, true, case_sensitive)? {
            return Ok(false);
        }
        left -= 1;
        if left > 0 {
            child = skip(input, child, true)?;
        }
    }
    Ok(true)
}

/// The original's `cJSON_Compare` on two items that are not the same item.
fn compare(input: &[u8], a: usize, a_named: bool, b: usize, b_named: bool, case_sensitive: bool) -> Result<bool, ()> {
    let kind = *input.get(a).ok_or(())?;
    if kind != *input.get(b).ok_or(())? || !known(kind) {
        return Ok(false);
    }
    let a_body = body_of(input, a, a_named)?;
    let b_body = body_of(input, b, b_named)?;
    match kind {
        NUMBER => Ok(doubles_equal(double_at(input, a_body)?, double_at(input, b_body)?)),
        STRING | RAW => match (text_at(input, a_body)?.0, text_at(input, b_body)?.0) {
            (Some(a_text), Some(b_text)) => Ok(a_text == b_text),
            _ => Ok(false),
        },
        ARRAY => {
            let a_count = count_at(input, a_body)?;
            let b_count = count_at(input, b_body)?;
            let mut a_child = a_body + 2;
            let mut b_child = b_body + 2;
            let mut left = if a_count < b_count { a_count } else { b_count };
            while left > 0 {
                if !compare(input, a_child, false, b_child, false, case_sensitive)? {
                    return Ok(false);
                }
                left -= 1;
                if left > 0 {
                    a_child = skip(input, a_child, false)?;
                    b_child = skip(input, b_child, false)?;
                }
            }
            Ok(a_count == b_count)
        }
        OBJECT => {
            let a_count = count_at(input, a_body)?;
            let b_count = count_at(input, b_body)?;
            let a_first = a_body + 2;
            let b_first = b_body + 2;
            Ok(each_found(input, a_first, a_count, b_first, b_count, case_sensitive)?
                && each_found(input, b_first, b_count, a_first, a_count, case_sensitive)?)
        }
        _ => Ok(true),
    }
}
