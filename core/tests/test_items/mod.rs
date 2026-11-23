//! Source text with every `#[cfg(test)]` item blanked, lines kept in place.

const TEST_ONLY: &[u8] = b"#[cfg(test)]";
const ITEM_WORDS: [&str; 16] = [
    "fn",
    "async",
    "const",
    "static",
    "mod",
    "struct",
    "enum",
    "union",
    "impl",
    "trait",
    "type",
    "use",
    "extern",
    "unsafe",
    "let",
    "macro_rules",
];

pub fn without_test_items(source: &str) -> String {
    let mask = masked(source.as_bytes());
    let mut output = source.as_bytes().to_vec();
    let mut from = 0;
    while let Some(found) = mask[from..]
        .windows(TEST_ONLY.len())
        .position(|window| window == TEST_ONLY)
    {
        let start = from + found;
        let end = item_end(&mask, start + TEST_ONLY.len());
        for byte in &mut output[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        from = end;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn item_end(mask: &[u8], from: usize) -> usize {
    let listed = !starts_item(&mask[from..]);
    let mut depth = 0_i32;
    for (offset, byte) in mask[from..].iter().enumerate() {
        let here = from + offset;
        match byte {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                if depth == 0 {
                    return here;
                }
                depth -= 1;
                if *byte == b'}' && depth == 0 {
                    return here + 1;
                }
            }
            b';' if depth == 0 => return here + 1,
            b',' if depth == 0 && listed => return here + 1,
            _ => {}
        }
    }
    mask.len()
}

fn starts_item(rest: &[u8]) -> bool {
    let mut at = 0;
    loop {
        while rest.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if rest.get(at) != Some(&b'#') {
            break;
        }
        let mut depth = 0_i32;
        while let Some(byte) = rest.get(at) {
            at += 1;
            match byte {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let mut word = next_word(rest, &mut at);
    if word == "pub" {
        while rest.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if rest.get(at) == Some(&b'(') {
            while rest.get(at).is_some_and(|byte| *byte != b')') {
                at += 1;
            }
            at += 1;
        }
        word = next_word(rest, &mut at);
    }
    ITEM_WORDS.contains(&word.as_str())
}

fn next_word(rest: &[u8], at: &mut usize) -> String {
    while rest.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
    let start = *at;
    while rest
        .get(*at)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        *at += 1;
    }
    String::from_utf8_lossy(&rest[start..*at]).into_owned()
}

fn masked(source: &[u8]) -> Vec<u8> {
    let mut mask = source.to_vec();
    let mut at = 0;
    while at < source.len() {
        let end = match (source[at], source.get(at + 1)) {
            (b'/', Some(b'/')) => source[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(source.len(), |found| at + found),
            (b'/', Some(b'*')) => block_comment_end(source, at),
            (b'r' | b'b', _) if starts_raw_string(source, at) => raw_string_end(source, at),
            (b'"', _) => quoted_end(source, at, b'"'),
            (b'\'', _) if starts_character(source, at) => quoted_end(source, at, b'\''),
            _ => {
                at += 1;
                continue;
            }
        };
        for byte in &mut mask[at..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        at = end;
    }
    mask
}

fn block_comment_end(source: &[u8], start: usize) -> usize {
    let mut depth = 0_u32;
    let mut at = start;
    while at + 1 < source.len() {
        match (source[at], source[at + 1]) {
            (b'/', b'*') => {
                depth += 1;
                at += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                at += 2;
                if depth == 0 {
                    return at;
                }
            }
            _ => at += 1,
        }
    }
    source.len()
}

fn starts_raw_string(source: &[u8], at: usize) -> bool {
    if at > 0 && (source[at - 1].is_ascii_alphanumeric() || source[at - 1] == b'_') {
        return false;
    }
    let mut next = at + usize::from(source[at] == b'b');
    if source.get(next) != Some(&b'r') {
        return false;
    }
    next += 1;
    while source.get(next) == Some(&b'#') {
        next += 1;
    }
    source.get(next) == Some(&b'"')
}

fn raw_string_end(source: &[u8], at: usize) -> usize {
    let open = at
        + source[at..]
            .iter()
            .position(|byte| *byte == b'"')
            .unwrap_or(0);
    let hashes = source[at..open]
        .iter()
        .filter(|byte| **byte == b'#')
        .count();
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    source[open + 1..]
        .windows(closing.len())
        .position(|window| window == closing.as_slice())
        .map_or(source.len(), |found| open + 1 + found + closing.len())
}

fn quoted_end(source: &[u8], start: usize, quote: u8) -> usize {
    let mut at = start + 1;
    while at < source.len() {
        match source[at] {
            b'\\' => at += 2,
            byte if byte == quote => return at + 1,
            _ => at += 1,
        }
    }
    source.len()
}

fn starts_character(source: &[u8], at: usize) -> bool {
    match source.get(at + 1) {
        Some(b'\\') => true,
        Some(b'\'' | b'\n' | b'\r') | None => false,
        Some(first) => {
            let width = match first {
                0x00..=0x7f => 1,
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                _ => 4,
            };
            source.get(at + 1 + width) == Some(&b'\'')
        }
    }
}
