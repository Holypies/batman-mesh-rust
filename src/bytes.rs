
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


#[cfg(test)]
mod tests{
    use super::*;


    // --- tests for u16 ---
    #[test]
    fn read_u16_test(){
        let buf= [0x12, 0x34];
        let read = read_u16(&buf, 0);
        assert_eq!(read, 0x1234);
    }

    #[test]
    fn read_u16_offset_test(){
        let buf= [0x12, 0x34, 0x56, 0x78, 0x9F];
        let read = read_u16(&buf, 1);
        assert_eq!(read, 0x3456);
    }

    #[test]
    fn write_u16_test(){
        let mut buf = [0x00,0x00];
        write_u16(&mut buf, 0, 0x1234);
        assert_eq!(buf, [0x12,0x34]);
    }

    #[test]
    fn write_u16_offset_test(){
        let mut buf= [0x12, 0x34, 0x56, 0x78, 0x9F];
        write_u16(&mut buf, 1,0x5678);
        assert_eq!(buf, [0x12, 0x56, 0x78, 0x78, 0x9F]);
    }

    #[test]
    fn read_u16_extremes() {
        assert_eq!(read_u16(&[0x00, 0x00], 0), 0);
        assert_eq!(read_u16(&[0xFF, 0xFF], 0), u16::MAX);
        assert_eq!(read_u16(&[0x00, 0x01], 0), 1);
        assert_eq!(read_u16(&[0x01, 0x00], 0), 256);
    }

    // --- tests for u32 ---
    #[test]
    fn read_u32_test(){
        let buf= [0x12, 0x34, 0x56, 0x78];
        let read = read_u32(&buf, 0);
        assert_eq!(read, 0x12345678);
    }

    #[test]
    fn read_u32_offset_test(){
        let buf= [0x12, 0x34, 0x56, 0x78, 0x9F];
        let read = read_u32(&buf, 1);
        assert_eq!(read, 0x3456789F);
    }

    #[test]
    fn write_u32_test(){
        let mut buf = [0x00,0x00,0x00,0x00];
        write_u32(&mut buf, 0, 0x12345678);
        assert_eq!(buf, [0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn write_u32_offset_test(){
        let mut buf= [0x12, 0x34, 0x56, 0x78, 0x9F];
        write_u32(&mut buf, 1,0x12345678);
        assert_eq!(buf, [0x12, 0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn read_u32_extremes() {
        assert_eq!(read_u32(&[0x00, 0x00, 0x00, 0x00], 0), 0);
        assert_eq!(read_u32(&[0xFF, 0xFF, 0xFF, 0xFF], 0), u32::MAX);
        assert_eq!(read_u32(&[0x00, 0x00, 0x00, 0x01], 0), 1);
        assert_eq!(read_u32(&[0x01, 0x00, 0x00, 0x00], 0), 1 << 24);
    }

    // --- panic tests u16 ---
    #[test]
    #[should_panic]
    fn read_u16_panics_only_one_byte_left() {
        let buf = [0x00, 0x00, 0x00];
        read_u16(&buf, 2);
    }
    #[test]
    #[should_panic]
    fn read_u16_panics_empty_buffer() {
        read_u16(&[], 0);
    }
 
    #[test]
    #[should_panic]
    fn read_u16_panics_offset_overflow() {
        let buf = [0x00, 0x00];
        read_u16(&buf, usize::MAX);
    }

    // --- panic tests u32 ---

    #[test]
    #[should_panic]
    fn read_u32_panics_only_three_bytes_left() {
        let buf = [0u8; 5];
        read_u32(&buf, 2);
    }
 
    #[test]
    #[should_panic]
    fn read_u32_panics_empty_buffer() {
        read_u32(&[], 0);
    }
 
    #[test]
    #[should_panic]
    fn read_u32_panics_offset_overflow() {
        let buf = [0u8; 4];
        read_u32(&buf, usize::MAX - 1);
    }

}