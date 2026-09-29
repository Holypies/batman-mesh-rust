
pub(crate) fn read_u16(buf: &[u8], off: usize) -> u16 {
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(&buf[off..off + 2]);
    u16::from_be_bytes(bytes)
}

pub(crate) fn write_u16(buf: &mut [u8], off: usize, value: u16) {
    buf[off..off + 2].copy_from_slice(&value.to_be_bytes());
}

pub(crate) fn read_u32(buf: &[u8], off: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&buf[off..off + 4]);
    u32::from_be_bytes(bytes)
}

pub(crate) fn write_u32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_be_bytes());
}