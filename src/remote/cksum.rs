//! The POSIX `cksum` checksum, so a file's fingerprint can be worked out
//! here from its bytes and compared with `cksum` run on the host.

/// CRC-32 with polynomial 0x04C11DB7, most significant bit first.
const POLYNOMIAL: u32 = 0x04C1_1DB7;

/// What `cksum < file` prints for `bytes`: the checksum and the length.
pub fn cksum_text(bytes: &[u8]) -> String {
    format!("{} {}", cksum(bytes), bytes.len())
}

/// The POSIX `cksum` value: the CRC of the bytes, then of the length in as
/// few bytes as it needs (lowest byte first), then inverted.
pub fn cksum(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    let mut feed = |byte: u8| {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 { crc << 1 } else { (crc << 1) ^ POLYNOMIAL };
        }
    };
    for &byte in bytes {
        feed(byte);
    }
    let mut length = bytes.len() as u64;
    while length > 0 {
        #[expect(clippy::cast_possible_truncation, reason = "the length is fed one low byte at a time")]
        feed(length as u8);
        length >>= 8;
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_values() {
        // From GNU coreutils and BSD `cksum`.
        assert_eq!(cksum_text(b""), "4294967295 0");
        assert_eq!(cksum_text(b"hello\n"), "3015617425 6");
    }

    #[test]
    fn matches_the_system_cksum() {
        let input = "# Notes\r\nsome text ✓\n\n".repeat(300);
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("printf '%s' \"$1\" | cksum")
            .arg("sh")
            .arg(&input)
            .output()
            .unwrap();
        let system = String::from_utf8(output.stdout).unwrap();
        assert_eq!(system.trim(), cksum_text(input.as_bytes()));
    }
}
