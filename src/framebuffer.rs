use crate::consts::{MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM, BCAST_HEADROOM, OGM_HEADROOM, MIN_ETH_FRAME_SIZE};

// frame consts
//pub const MESH_HEADROOM: usize = ETH_HLEN+WIRE_LEN;
//pub const MAX_LINK_PAYLOAD:usize = 1504;
//pub const MESH_FRAME_CAP:usize = MESH_HEADROOM+ETH_HLEN+MAX_LINK_PAYLOAD;

// frame types
// TODO: add correct payload for MeshFrame
pub type UnicastFrame = FrameBuffer<MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM>;
pub type BroadcastFrame = FrameBuffer<MAX_BATMAN_FRAME_SIZE, BCAST_HEADROOM>;
pub type OgmFrame = FrameBuffer<MIN_ETH_FRAME_SIZE,OGM_HEADROOM>;



#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError{
    NoHeadRoom{need:usize, got:usize},
    NoTailRoom{need:usize, got:usize},
    TooShort{need: usize, got:usize},
    NoCapacity{need:usize, cap:usize},
    TooLarge,
}

pub struct FrameBuffer<const N:usize, const HEADROOM:usize>{
    data: [u8; N],
    start: usize,
    end: usize,
}

impl<const N:usize,const HEADROOM:usize> FrameBuffer<N, HEADROOM>{
    
    const HEADROOM_FITS: () =
        assert!(HEADROOM <= N, "FrameBuffer headroom exceeds its capacity");

    /// Creates a new empty frame buffer of size 'N', with pointers at 'headroom'
    pub const fn new() -> Self {
        let () = Self::HEADROOM_FITS;
        Self {
            data: [0u8; N],
            start: HEADROOM,
            end: HEADROOM,
        }
    }
    
    /// returns length of the current window
    pub fn len(&self)-> usize{
        self.end-self.start
    }
    /// returns the headroom of the ethernet head
    pub fn headroom(&self)-> usize{
        self.start
    }
    /// returns the tailroom of the enthernet head
    pub fn tailroom(&self)-> usize{
        N-self.end
    }
    /// returns the bytes in window as inmutable
    pub fn bytes(&self)-> &[u8]{
        &self.data[self.start..self.end]
    }
    /// returns the bytes in window as mutable
    pub fn bytes_mut(&mut self)->&mut [u8]{
        &mut self.data[self.start..self.end]
    }

    /// moves the start cursor by 'len' and returns oclets to be filled
    pub fn prepend(&mut self, len:usize)-> Result<&mut[u8], FrameError>{
        if len > self.start{
            return Err(FrameError::NoHeadRoom{need:len, got:self.start});
        }
        self.start -= len;
        Ok(
            &mut self.data[self.start..self.start+len]
        )
    }

    /// Moves 'len' oclets from the front and moves the start cursor back
    /// 'len' oclets, returning the headroom
    pub fn strip(&mut self, len:usize)->Result<(),FrameError>{
        if len>self.len(){
            return Err(FrameError::TooShort{need:len, got:self.len()});
        }
        self.start += len;
        Ok(())
    }

    pub fn truncate(&mut self, len:usize)->Result<(),FrameError>{
        if len>self.len(){
            return Err(FrameError::TooShort{need:len, got:self.len()});
        }
        self.end =  self.start+len;
        Ok(())
    }

    /// reserves 'len' oclets from the end and returns them to be filled 
    pub fn append(&mut self, len:usize)->Result<&mut [u8],FrameError>{
        if len > self.tailroom(){
            return Err(FrameError::NoTailRoom{need:len, got:self.tailroom()});
        }
        let current_index = self.end;
        self.end += len;
        Ok(
            &mut self.data[current_index..self.end]
        )
    }

    /// resets cursors to 'headroom'
    pub fn reset_cursors(&mut self) {
        self.start = HEADROOM;
        self.end = HEADROOM;
    }
    /// empties the frame buffer data
    pub fn reset_frame_data(&mut self){
        self.data = [0u8; N];
    }

    pub fn load_frame(&mut self, frame:&[u8])->Result<(), FrameError>{
        let available_space = N-HEADROOM;
        if frame.len()>available_space{
             return Err(FrameError::NoHeadRoom{need:frame.len(), got:available_space});
        }
        self.start = HEADROOM;
        self.end = HEADROOM+frame.len();
        self.data[self.start..self.end].copy_from_slice(frame);
        Ok(())
    }

    pub fn rx_space(&mut self)->&mut [u8]{
        self.reset_cursors();
        &mut self.data[HEADROOM..N]
    }

    pub fn commit(&self, com:&[u8])->Result<(), FrameError>{
        if self.start == HEADROOM && self.end == HEADROOM{
            let cap: usize = N - HEADROOM;
            if com.len()>cap{
                return Err(FrameError::NoCapacity{need:com.len(), cap:cap as usize});
            }

        }
        return Err(FrameError::TooShort{need:com.len(), got:self.len()});
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    // --- test getters ---
    #[test]
    fn frame_buffer_constructor_test(){
        let buf_new: FrameBuffer<8, 4> = FrameBuffer::new();
        
        assert_eq!(buf_new.data, [0u8;8]);
        assert_eq!(buf_new.start, 4);
        assert_eq!(buf_new.end, 4);

    }

    #[test]
    fn frame_buffer_len_test(){
        let mut buf_new: FrameBuffer<8, 14> = FrameBuffer::new();
        // empty
        assert_eq!(buf_new.len(), 0);

        // len of 16
        buf_new.end = 16;
        assert_eq!(buf_new.len(), 16);

        // len of 2-16 = 14
        buf_new.start = 2;
        assert_eq!(buf_new.len(), 14);

        // len of 2-2 = 0
        buf_new.end = 2;
        assert_eq!(buf_new.len(), 0);

    }

    #[test]
    fn frame_buffer_headroom_test(){
        let buf_new: FrameBuffer<8, 14> = FrameBuffer::new();
        assert_eq!(buf_new.headroom(), 14);

        // --- edge cases ---
        // no headroom
        let no_head: FrameBuffer<8, 0> = FrameBuffer::new();
        assert_eq!(no_head.headroom(), 0);

        // all headroom
        let all_head: FrameBuffer<8, 8> = FrameBuffer::new();
        assert_eq!(all_head.headroom(), 8);
    }

    #[test]
    fn frame_buffer_tailroom_test(){
        let buf_new: FrameBuffer<8, 14> = FrameBuffer::new();
        assert_eq!(buf_new.tailroom(), 22);

        // --- edge cases ---
        // no headroom
        let no_head: FrameBuffer<8, 0> = FrameBuffer::new();
        assert_eq!(no_head.tailroom(), 8);

        // all headroom
        let all_head: FrameBuffer<8, 8> = FrameBuffer::new();
        assert_eq!(all_head.tailroom(), 0);
    }

    #[test]
    fn frame_buffer_bytes_test(){
        let mut buf_new: FrameBuffer<16, 4> = FrameBuffer::new();
        assert!(buf_new.bytes().is_empty());

        // fill backing array with known pattern: data[i] = i
        for (i, b) in buf_new.data.iter_mut().enumerate() {
            *b = i as u8;
        }

        // empty window still empty even with data present
        assert!(buf_new.bytes().is_empty());

        buf_new.start = 4;
        buf_new.end = 10;
        assert_eq!(buf_new.bytes(), &[4u8, 5, 6, 7, 8, 9]);

        // full window
        buf_new.start = 0;
        buf_new.end = 16;
        assert_eq!(buf_new.bytes().len(), 16);
        assert_eq!(buf_new.bytes(), &buf_new.data[..]);


        // single byte at start
        buf_new.start = 0;
        buf_new.end = 1;
        assert_eq!(buf_new.bytes(), &[0u8]);

        // single byte at end
        buf_new.start = 15;
        buf_new.end = 16;
        assert_eq!(buf_new.bytes(), &[15u8]);
       
    }

    #[test]
    fn frame_buffer_bytes_mut_test(){
        let mut buf_new: FrameBuffer<16, 4> = FrameBuffer::new();
        assert!(buf_new.bytes_mut().is_empty());

        buf_new.start = 4;
        buf_new.end = 8;
        assert_eq!(buf_new.bytes_mut().len(), 4);

        buf_new.bytes_mut().copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        // writes land inside window
        assert_eq!(&buf_new.data[4..8], &[0xAAu8, 0xBB, 0xCC, 0xDD]);

        // bytes outside window untouched
        assert_eq!(&buf_new.data[..4], &[0u8; 4]);
        assert_eq!(&buf_new.data[8..], &[0u8; 8]);

        // bytes() sees same data
        assert_eq!(buf_new.bytes(), &[0xAAu8, 0xBB, 0xCC, 0xDD]);

        // single-index write
        buf_new.bytes_mut()[0] = 0x11;
        assert_eq!(buf_new.data[4], 0x11);
    }

    // --- test functions ---
    #[test]
    fn frame_buffer_prepend_test(){
        let mut buf_new: FrameBuffer<16, 4> = FrameBuffer::new();
        buf_new.data[4..8].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        // check error if to large
        let r = buf_new.prepend(5);
        assert_eq!(r, Err(FrameError::NoHeadRoom { need: 5, got: 4 }));

        // check if prepend with length 0 zeros
        let a = buf_new.prepend(0);
        assert!(a.unwrap().is_empty());

        // check regular functionality 
        let b = buf_new.prepend(2).unwrap();
        b.copy_from_slice(&[ 0x22, 0x33]);
        assert_eq!(&buf_new.data[2..4], &[0x22,0x33]);
        
        // check exact remaining headroom
        buf_new.start=4;
        let c  = buf_new.prepend(4).unwrap();
        c.copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&buf_new.data[0..4], [0xAA, 0xBB, 0xCC, 0xDD]);

        // check if no space available
        buf_new.start = 0;
        let r = buf_new.prepend(1);
        assert_eq!(r, Err(FrameError::NoHeadRoom { need: 1, got: 0 }));
        
        
    }
    
    #[test]
    fn frame_buffer_truncate_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        buf.end = 8;
        buf.data[4..8].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(buf.start,4);
        assert_eq!(buf.end, 8);

        // check if it trucates by len
        let _r = buf.truncate(2);
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 6);

        // check if it errors when not enough space to trunc
        buf.end = 5;
        let r = buf.truncate(2);
        assert_eq!(r, Err(FrameError::TooShort{need:2, got:1}));

        // check what happens when len = 0
        let _r = buf.truncate(0);
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 5);
    }
    #[test]
    fn frame_buffer_strip_test(){
         let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        for (i, b) in buf.data.iter_mut().enumerate() {
            *b = i as u8;
        }
        buf.end = 10;

        // zero-length
        assert_eq!(buf.strip(0), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 10);

        // partial, start moves forward, end fixed
        assert_eq!(buf.strip(2), Ok(()));
        assert_eq!(buf.start, 6);
        assert_eq!(buf.end, 10);
        assert_eq!(buf.headroom(), 6);
        assert_eq!(buf.bytes(), &[6u8, 7, 8, 9]);

        // larger than window
        assert_eq!(buf.strip(5), Err(FrameError::TooShort { need: 5, got: 4 }));
        assert_eq!(buf.start, 6);
        assert_eq!(buf.end, 10);

        // exact window length
        assert_eq!(buf.strip(4), Ok(()));
        assert_eq!(buf.start, 10);
        assert_eq!(buf.end, 10);
        assert!(buf.bytes().is_empty());

        // empty window: any nonzero strip fails
        assert_eq!(buf.strip(1), Err(FrameError::TooShort { need: 1, got: 0 }));

        // data never modified
        for (i, b) in buf.data.iter().enumerate() {
            assert_eq!(*b, i as u8);
        }

    }
    #[test]
    fn frame_buffer_append_test(){
        // too large on fresh buffer
        let mut fresh: FrameBuffer<16, 4> = FrameBuffer::new();
        assert_eq!(fresh.append(13), Err(FrameError::NoTailRoom { need: 13, got: 12 }));
        assert_eq!(fresh.start, 4);
        assert_eq!(fresh.end, 4);

        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();

        // zero-length
        {
            let s = buf.append(0).unwrap();
            assert!(s.is_empty());
        }
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);

        let s = buf.append(3).unwrap();
        assert_eq!(s.len(), 3);
        s.copy_from_slice(&[1, 2, 3]);
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 7);
        assert_eq!(&buf.data[4..7], &[1u8, 2, 3]);

        // second append contiguous with first
        let s = buf.append(2).unwrap();
        s.copy_from_slice(&[4, 5]);
        assert_eq!(buf.end, 9);
        assert_eq!(buf.bytes(), &[1u8, 2, 3, 4, 5]);

        // headroom region untouched
        assert_eq!(&buf.data[..4], &[0u8; 4]);

        // exact remaining tailroom (16 - 9 = 7)
        let s = buf.append(7).unwrap();
        assert_eq!(s.len(), 7);
        assert_eq!(buf.end, 16);
        assert_eq!(buf.tailroom(), 0);

        // no tailroom left
        assert_eq!(buf.append(1), Err(FrameError::NoTailRoom { need: 1, got: 0 }));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 16);
    }
    #[test]
    fn frame_buffer_reset_cursors_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        for (i, b) in buf.data.iter_mut().enumerate() {
            *b = i as u8;
        }
        buf.start = 1;
        buf.end = 12;

        buf.reset_cursors();
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.headroom(), 4);
        assert_eq!(buf.tailroom(), 12);

        
        buf.reset_cursors();
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);

        // data untouched
        for (i, b) in buf.data.iter().enumerate() {
            assert_eq!(*b, i as u8);
        }
    }
    #[test]
    fn frame_buffer_reset_frame_data_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        // all nonzero so zeroing is visible at every index
        for (i, b) in buf.data.iter_mut().enumerate() {
            *b = (i as u8) + 1;
        }
        buf.start = 2;
        buf.end = 9;

        buf.reset_frame_data();
        assert_eq!(buf.data, [0u8; 16]);

        // cursors untouched
        assert_eq!(buf.start, 2);
        assert_eq!(buf.end, 9);
    }
    #[test]
    fn frame_buffer_load_frame_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        let frame = [0xDEu8, 0xAD, 0xBE, 0xEF, 0x01];

        assert_eq!(buf.load_frame(&frame), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 9);
        assert_eq!(buf.bytes(), &frame);
        // outside window untouched
        assert_eq!(&buf.data[..4], &[0u8; 4]);
        assert_eq!(&buf.data[9..], &[0u8; 7]);

        // reload after cursors moved: start back to HEADROOM, window replaced
        buf.start = 1;
        buf.end = 15;
        assert_eq!(buf.load_frame(&[7, 8]), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 6);
        assert_eq!(buf.bytes(), &[7u8, 8]);

        // exactly full capacity (N - HEADROOM = 12)
        let full = [0xFFu8; 12];
        assert_eq!(buf.load_frame(&full), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 16);
        assert_eq!(buf.tailroom(), 0);
        assert_eq!(buf.bytes(), &full);

        // empty frame
        assert_eq!(buf.load_frame(&[]), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);
    }

    #[test]
    fn frame_buffer_load_frame_too_large_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        assert_eq!(buf.load_frame(&[1, 2, 3]), Ok(()));

        let big = [0xAAu8; 13];
        // frame exceeds N - HEADROOM: capacity error, same variant commit() uses
        assert_eq!(buf.load_frame(&big), Err(FrameError::NoCapacity { need: 13, cap: 12 }));

        // previous frame intact
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 7);
        assert_eq!(buf.bytes(), &[1u8, 2, 3]);
    }
    #[test]
    fn frame_buffer_rx_space_test(){
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        buf.start = 1;
        buf.end = 12;


        let rx = buf.rx_space();
        assert_eq!(rx.len(), 12); // N - HEADROOM
        rx[0] = 0xAA;
        rx[11] = 0xBB;

        // cursors reset, nothing committed yet
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);

        // slice maps to data[HEADROOM..N]
        assert_eq!(buf.data[4], 0xAA);
        assert_eq!(buf.data[15], 0xBB);
        assert_eq!(&buf.data[..4], &[0u8; 4]);

        // HEADROOM == N: no rx space
        let mut no_space: FrameBuffer<8, 8> = FrameBuffer::new();
        assert!(no_space.rx_space().is_empty());
    }

    #[test]
    fn frame_buffer_commit_too_large_test(){
        let buf: FrameBuffer<16, 4> = FrameBuffer::new();
        assert_eq!(buf.commit(&[0u8; 13]), Err(FrameError::NoCapacity { need: 13, cap: 12 }));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 4);
    }

    #[test]
    fn frame_buffer_commit_fits_test(){
       
        let mut buf: FrameBuffer<16, 4> = FrameBuffer::new();
        let rx = buf.rx_space();
        rx[..3].copy_from_slice(&[1, 2, 3]);
        assert_eq!(buf.commit(&[1, 2, 3]), Ok(()));
        assert_eq!(buf.start, 4);
        assert_eq!(buf.end, 7);
        assert_eq!(buf.bytes(), &[1u8, 2, 3]);

        // exact capacity
        let mut full: FrameBuffer<16, 4> = FrameBuffer::new();
        full.rx_space();
        assert_eq!(full.commit(&[0u8; 12]), Ok(()));
        assert_eq!(full.len(), 12);
    }
}

