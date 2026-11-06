//! Fabric's bundled JsonReader accepts literal control characters inside quoted
//! strings (notably multiline descriptions), even with lenient mode disabled.
//! Preserve those characters while leaving structural/escape errors invalid.
//! See FabricMC/fabric-loader: impl/lib/gson/JsonReader.nextQuotedValue.
use serde_json::Value;

pub(crate) fn parse(bytes: &[u8]) -> serde_json::Result<Value> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let original = match serde_json::from_slice(bytes) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let mut normalized = Vec::with_capacity(bytes.len());
    let (mut quoted, mut escaped, mut changed) = (false, false, false);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in bytes {
        if quoted && byte < 0x20 && (!escaped || byte == b'\n') {
            if escaped {
                normalized.pop();
            }
            normalized.extend_from_slice(&[
                b'\\',
                b'u',
                b'0',
                b'0',
                HEX[(byte >> 4) as usize],
                HEX[(byte & 15) as usize],
            ]);
            escaped = false;
            changed = true;
        } else if quoted && escaped {
            // JsonReader also accepts \' inside double-quoted strings.
            if byte == b'\'' {
                normalized.pop();
                changed = true;
            }
            normalized.push(byte);
            escaped = false;
        } else {
            if byte == b'"' {
                quoted = !quoted;
            } else if quoted && byte == b'\\' {
                escaped = true;
            }
            normalized.push(byte);
        }
    }
    if changed {
        serde_json::from_slice(&normalized)
    } else {
        Err(original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiline_descriptions_preserve_constraints_and_text() {
        let value = parse(b"{\"id\":\"bwncr\",\"description\":\"First line\nSecond\tline\r\n\",\"depends\":{\"minecraft\":\"~1.21\",\"cloth-config\":\">=15\"}}").unwrap();
        assert_eq!(value["description"], "First line\nSecond\tline\r\n");
        assert_eq!(value["depends"]["minecraft"], "~1.21");
        assert_eq!(value["depends"]["cloth-config"], ">=15");
        assert_eq!(
            parse(b"{\"text\":\"one\\\ntwo\"}").unwrap()["text"],
            "one\ntwo"
        );
        assert_eq!(
            parse(br#"{"text":"It\'s valid"}"#).unwrap()["text"],
            "It's valid"
        );
    }
    #[test]
    fn regular_escapes_and_json_structure_are_not_reinterpreted() {
        let bytes = br#"{"text":"quotes: \" newline: \n slash: \\ ","path":"C:\\mods\\file.jar","array":[1,true,null]}"#;
        assert_eq!(
            parse(bytes).unwrap(),
            serde_json::from_slice::<Value>(bytes).unwrap()
        );
        for bytes in [
            b"{\"a\":1\x01}".as_slice(),
            b"{\"a\":\"bad\\q\ntext\"}",
            b"{\"a\":\"unfinished\n",
            b"{\"a\":\"line\nbreak\",}",
            b"{\"a\":\"line\nbreak\"} {}",
        ] {
            assert!(parse(bytes).is_err());
        }
        assert_eq!(parse(b"\xef\xbb\xbf{\"id\":\"cat\"}").unwrap()["id"], "cat");
    }
}
