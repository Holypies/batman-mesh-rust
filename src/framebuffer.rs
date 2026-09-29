use crate::consts::{ETH_HLEN, HEADROOM};
use crate::Mac::Mac;

// frame consts
pub const MESH_HEADROOM: usize = ETH_HLEN+WIRE_LEN;
pub const MAX_LINK_PAYLOAD:usize = 1504;
pub const MESH_FRAME_CAP:usize = MESH_HEADROOM+ETH_HLEN+MAX_LINK_PAYLOAD;

// frame types
// TODO: add correct payload for MeshFrame
pub type MeshFrame = FrameBuffer<1552,34>;
pub type OgmFrame = FrameBuffer<256,14>;



#[derive(Debug, Clone, Copy)]
pub enum FrameError{
    NoHeadRoom{need:usize, got:usize},
    NoTailRoom{need:usize, got:usize},
    TooShort{need: usize, got:usize},
    NoCapacity{need:usize, cap:usize},
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

    pub fn commit(&self, com:&[u8]){
        if self.start == HEADROOM && self.end == HEADROOM{
            let cap: usize = N - HEADROOM;
            if com>cap{
                return Err(FrameBuffer::NoCapacity{need:com, cap},);
            }

        }
        return Err(FrameError::TooShort{need:com.len(), got:self.len()});
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_buffer_constructor_test(){
        let buf: FrameBuffer<8, 4> = FrameBuffer::new();


    }

    #[test]
    fn frame_buffer_len_test(){
        
    }
    #[test]
    fn frame_buffer_headroom_test(){

    }
    #[test]
    fn frame_buffer_tailroom_test(){

    }
    #[test]
    fn frame_buffer_bytes_test(){

    }
    #[test]
    fn frame_buffer_bytes_mut_test(){

    }
}

