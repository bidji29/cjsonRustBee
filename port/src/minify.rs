//! The leaf `minify`: a JSON text without its white space and comments, as cJSON's `cJSON_Minify` answers.
//!
//! The original works in place on a text ended by a zero byte; here the end of the input plays the part of
//! that zero byte, and the answer is written to `out`. The quirks are kept: a `/` that opens no comment is
//! dropped, a comment or a string left open runs to the end of the text, and inside a string a backslash
//! protects only a following quote.

/// The byte at `at`, or the zero byte that ends the original's text.
fn byte(input: &[u8], at: usize) -> u8 {
    match input.get(at) {
        Some(&b) => b,
        None => 0,
    }
}

fn put(out: &mut [u8], at: usize, b: u8) -> Result<(), ()> {
    match out.get_mut(at) {
        Some(slot) => {
            *slot = b;
            Ok(())
        }
        None => Err(()),
    }
}

pub fn answer(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if input.iter().any(|&b| b == 0) {
        return Err(());
    }
    let n = input.len();
    let mut i = 0usize;
    let mut o = 0usize;
    while i < n {
        match input[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'/' => {
                let next = byte(input, i + 1);
                if next == b'/' {
                    i += 2;
                    while i < n {
                        if input[i] == b'\n' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                } else if next == b'*' {
                    i += 2;
                    while i < n {
                        if input[i] == b'*' && byte(input, i + 1) == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'"' => {
                put(out, o, b'"')?;
                i += 1;
                o += 1;
                while i < n {
                    let b = input[i];
                    put(out, o, b)?;
                    if b == b'"' {
                        i += 1;
                        o += 1;
                        break;
                    } else if b == b'\\' && byte(input, i + 1) == b'"' {
                        put(out, o + 1, b'"')?;
                        i += 1;
                        o += 1;
                    }
                    i += 1;
                    o += 1;
                }
            }
            b => {
                put(out, o, b)?;
                i += 1;
                o += 1;
            }
        }
    }
    Ok(o)
}
