


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Structure a array vector with N spots of memory
pub struct ArrayVec<T, const N:usize>{
    data: [Option<T>; N],
    len: usize,
}

impl <T, const N:usize> ArrayVec<T, N>{
    /// Creates a array with N spots of memory upon
    /// initization
    pub const fn new() -> Self{
        Self{
            data: [const{None}; N],
            len: 0,
        }
    }

    /// Adds an element to the array and changes the 
    /// length of the struct 
    /// 
    /// Input:
    ///     x: Varible of same type as array
    /// 
    /// Returns:
    ///     If data array is full:
    ///         - Returns Error
    ///     Otherwise:
    ///         - returns ok
    pub fn push(&mut self, x:T)-> Result<usize,T>{
        if self.len >= N{
            return Err(x);
        }
        self.data[self.len] = Some(x);
        //let index:usize= self.len;
        self.len +=1;
        Ok(self.len)
    }

    /// Pops last element in list
    /// 
    /// Returns:
    ///     Last lement in list wrapped in Option
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        core::mem::replace(&mut self.data[self.len], None)
    }


    /// Gets an element from the array
    /// 
    /// Inputs:
    ///     index: index of element to return
    /// Return:
    ///     if index exists:
    ///         - Value of data at index as a refrence
    ///     if index doesn't exist:
    ///         - None
    /// 
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }    
        self.data[index].as_ref()
    } 

    /// Gets a mutable element from the array
    /// 
    /// Inputs:
    ///     index: index of element to return
    /// Return:
    ///     if index exists:
    ///         - Value of data at index as a refrence
    ///     if index doesn't exist:
    ///         - None
    /// 
    pub fn get_mut(&mut self, index:usize) -> Option<&mut T>{
        if index >= self.len {
            return None;
        }   
        self.data[index].as_mut()
    }

    /// Gets the length of the array
    /// 
    /// Return:
    ///     self.len as usize
    pub fn len(&self)->usize{
        self.len
    }

    /// 
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data[..self.len].iter().filter_map(Option::as_ref)
    }
 
    /// Like `iter`, but hands out mutable references.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.data[..self.len].iter_mut().filter_map(Option::as_mut)
    }
    
    /// Swap the element position of index and the last 
    /// element in list, and removes it from list
    /// 
    /// Inputs:
    ///     index: index of element to return
    /// Return:
    ///     if index exists:
    ///         - Value of data at index as uszie
    ///     if index doesn't exist:
    ///         - None
    /// 
    pub fn swap_remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        let last: usize = self.len - 1;
        self.data.swap(index, last);
        self.len = last;
        self.data[last].take()
    }

    /// Takes a object and checks if it exists in the list
    /// 
    /// Input:
    ///     obj: Generic type of the array
    /// Returns:
    ///     if obj exists:
    ///         - Index where obj exists as uszie
    ///     if obj doesnt exist:
    ///         - None 
    /// 
    pub fn position(&self, obj: impl FnMut(&T) -> bool) -> Option<usize> {
        self.iter().position(obj)
    }


    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let mut write = 0;
        for read in 0..self.len {
            debug_assert!(self.data[read].is_some(), "ArrayVec: hole below len");
            let kept = match self.data[read].as_mut() {
                Some(item) => keep(item),
                None => false,
            };
 
            if !kept {
                // Dropping the element leaves an empty slot for later
                // elements to move into.
                self.data[read] = None;
                continue;
            }
 
            if write != read {
                // Every slot from `write` up to `read` is already None, so
                // this moves the element down and leaves None behind.
                self.data.swap(write, read);
            }
            write += 1;
        }
        self.len = write;
    }

    pub fn is_full(&self)->bool{
        if self.len >= N {
            return true;
        }
        return false;
    }

    pub fn clear(&mut self){
        self.data = [const{None}; N];
        self.len = 0;
    }

}





#[cfg(test)]
mod tests {
    use super::ArrayVec;

    #[test]
    fn arrayvec_push_test(){
        let mut test_array: ArrayVec<i32, 1> = ArrayVec::new();

        let val = 32;
        let index = match test_array.push(val){
            Ok(x)=>x,
            Err(e)=>e as usize,
        };
        assert_eq!(index, 1);
        let val1 = 15;
        let index1 = match test_array.push(val1){
            Ok(x)=>x,
            Err(e)=>e as usize,
        };
        assert_eq!(index1, val1 as usize);
    }

    #[test]
    fn arrayvec_pop_test(){
        let mut test_array: ArrayVec<i32, 1> = ArrayVec { 
            data: ([Some(32)]), 
            len: (1) 
        };
        
        let a = match test_array.pop(){
            Some(val)=>val,
            Option::None=> 0,
        };
        assert_eq!(a,32);
        let b = match test_array.pop(){
            Some(val)=>val,
            Option::None=> 0,
        };
        assert_eq!(b,0);
    }

    #[test]
    fn arrayvec_get_test(){
        let test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };

        let a = match test_array.get(0){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(a,&32);

        let b = match test_array.get(1){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(b,&12);

        let c = match test_array.get(2){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(c,&0);

    }

    #[test]
    fn arrayvec_get_mut_test(){
        let mut test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };

        let a = match test_array.get_mut(0){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(a,&32);

        let b = match test_array.get_mut(1){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(b,&12);

        let c = match test_array.get_mut(2){
            Some(val)=>val,
            Option::None=>&0,
        };
        assert_eq!(c,&0);

    }

    #[test]
    fn arrayvec_len_test(){
        let test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        assert_eq!(test_array.len(),2);

    }
    #[test]
    fn arrayvec_iter_test(){
        let test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        let mut it = test_array.iter();
        assert_eq!(it.next(), Some(&32));
        assert_eq!(it.next(), Some(&12));
        assert_eq!(it.next(), None);

        // only yields elements below len
        let partial: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (1) 
        };
        assert_eq!(partial.iter().count(), 1);

        let empty: ArrayVec<i32, 2> = ArrayVec::new();
        assert_eq!(empty.iter().next(), None);
    }

    #[test]
    fn arrayvec_iter_mut_test(){
        let mut test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        for x in test_array.iter_mut() {
            *x *= 2;
        }
        assert_eq!(test_array.get(0), Some(&64));
        assert_eq!(test_array.get(1), Some(&24));

        // slots at or above len untouched
        let mut partial: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(1), Some(2)]), 
            len: (1) 
        };
        for x in partial.iter_mut() {
            *x += 10;
        }
        assert_eq!(partial.data, [Some(11), Some(2)]);
    }

    #[test]
    fn arrayvec_swap_remove_test(){
        let mut test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        test_array.swap_remove(0);
        assert_eq!(test_array.len,1);
       // assert_eq!(test_array.data, [Some(12)])
    }
    #[test]
    fn arrayvec_position_test(){
        let test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        assert_eq!(test_array.position(|&x| x == 32), Some(0));
        assert_eq!(test_array.position(|&x| x == 12), Some(1));
        assert_eq!(test_array.position(|&x| x == 15), None);

        // elements at or above len not searched
        let partial: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (1) 
        };
        assert_eq!(partial.position(|&x| x == 12), None);
    }
    #[test]
    fn arrayvec_is_full_test(){
        let test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32),None]), 
            len: (1) 
        };
        assert_eq!(test_array.is_full(), false);

        let test_array1: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32), Some(12)]), 
            len: (2) 
        };
        assert_eq!(test_array1.is_full(),true);
    }

    #[test]
    fn arrayvec_clear_test(){
        let mut test_array: ArrayVec<i32, 2> = ArrayVec { 
            data: ([Some(32),Some(12)]), 
            len: (2) 
        };
        test_array.clear();

        assert_eq!(test_array.data,[None,None]);
        assert_eq!(test_array.len, 0);
    }

}