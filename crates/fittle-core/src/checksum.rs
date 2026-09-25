//! FITS CHECKSUM / DATASUM (the checksum convention, Seaman et al.): a 32-bit
//! ones'-complement sum of big-endian words, encoded as 16 ASCII characters.

/// Ones'-complement sum of 4-byte big-endian words, folded with end-around carry.
pub fn sum32(bytes: &[u8], mut sum: u32) -> u32 {
    let mut acc = sum as u64;
    for w in bytes.chunks(4) {
        let mut b = [0u8; 4];
        b[..w.len()].copy_from_slice(w);
        acc += u32::from_be_bytes(b) as u64;
        if acc > u32::MAX as u64 {
            acc = (acc & 0xFFFF_FFFF) + (acc >> 32);
        }
    }
    while acc > u32::MAX as u64 {
        acc = (acc & 0xFFFF_FFFF) + (acc >> 32);
    }
    sum = acc as u32;
    sum
}

/// Add two ones'-complement sums.
pub fn add(a: u32, b: u32) -> u32 {
    let s = a as u64 + b as u64;
    ((s & 0xFFFF_FFFF) + (s >> 32)) as u32
}

const EXCLUDE: [u8; 13] = [
    0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60,
];

/// Encode a 32-bit value as the 16-character CHECKSUM string (the value is
/// the complement of the HDU sum).
pub fn encode(value: u32) -> String {
    let mut asc = [0u8; 16];
    for i in 0..4 {
        let byte = ((value >> ((3 - i) * 8)) & 0xFF) as i32;
        let quotient = byte / 4 + 0x30;
        let remainder = byte % 4;
        let mut ch = [quotient; 4];
        ch[0] += remainder;
        let mut check = true;
        while check {
            check = false;
            for k in EXCLUDE {
                for j in (0..4).step_by(2) {
                    if ch[j] as u8 == k || ch[j + 1] as u8 == k {
                        ch[j] += 1;
                        ch[j + 1] -= 1;
                        check = true;
                    }
                }
            }
        }
        for (j, c) in ch.iter().enumerate() {
            asc[4 * j + i] = *c as u8;
        }
    }
    // Rotate right by one.
    (0..16).map(|i| asc[(i + 15) % 16] as char).collect()
}

/// Place a CHECKSUM so the ones'-complement sum of `header` + `data_sum`
/// is -0. `header` is whole blocks and must contain a
/// `CHECKSUM= '0000000000000000'` card; returns the encoded string written.
pub fn seal(header: &mut [u8], data_sum: u32) -> Option<String> {
    let at = header
        .chunks(80)
        .position(|c| c.starts_with(b"CHECKSUM= '"))?
        * 80
        + 11;
    header[at..at + 16].copy_from_slice(b"0000000000000000");
    let total = add(sum32(header, 0), data_sum);
    let enc = encode(!total);
    header[at..at + 16].copy_from_slice(enc.as_bytes());
    Some(enc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_with_checksum() -> Vec<u8> {
        let cards = [
            "SIMPLE  =                    T",
            "BITPIX  =                   16",
            "NAXIS   =                    0",
            "CHECKSUM= '0000000000000000'",
            "DATASUM = '0         '",
            "END",
        ];
        let mut h: Vec<u8> = cards
            .iter()
            .flat_map(|c| format!("{c:<80}").into_bytes())
            .collect();
        h.resize(2880, b' ');
        h
    }

    #[test]
    fn sealed_hdu_sums_to_negative_zero() {
        for data_sum in [0u32, 1, 0x1234_5678, 0xFFFF_FFFE] {
            let mut h = header_with_checksum();
            let enc = seal(&mut h, data_sum).unwrap();
            assert!(enc.bytes().all(|b| b.is_ascii_alphanumeric()), "{enc}");
            assert_eq!(
                add(sum32(&h, 0), data_sum),
                0xFFFF_FFFF,
                "data_sum={data_sum:#x} enc={enc}"
            );
        }
    }
}
