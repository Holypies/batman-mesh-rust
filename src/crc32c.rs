/// The Castagnoli polynomial 0x1EDC6F41, bit-reversed for LSB-first processing
const POLY_REFLECTED: u32 = 0x82F6_3B78;

/// CRC32C computation from `seed` over `data`
pub const fn crc32c(seed: u32, data: &[u8]) -> u32 {
    let mut crc = seed;
    let mut i = 0;
    while i < data.len() {
        crc ^= data[i] as u32;
        let mut bit = 0;
        while bit < 8 {
            // All ones when the low bit is set, all zeros otherwise, so the
            // polynomial is applied without a branch
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (POLY_REFLECTED & mask);
            bit += 1;
        }
        i += 1;
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::crc32c;

}