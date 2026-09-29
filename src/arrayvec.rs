
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
        let index:usize= self.len;
        self.len +=1;
        Ok(index)
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
}





#[cfg(test)]
mod tests {
    use super::ArrayVec;

}