#![no_std]

use core::convert::TryInto;
extern crate alloc;
use alloc::vec::Vec;

#[cfg(test)]
mod tests;

#[derive(Copy)]
pub struct CircularBuffer<T> {
    buffer: *mut T,
    // writing pointer
    w: usize,
    // reading pointer
    r: usize,
    size: usize,
    full: bool,
}

impl<T> CircularBuffer<T> {
    /// Create a new CircularBuffer of size `size`.
    ///
    /// It allocate an array of exactly size element, if the allocation fail, the method panic.
    ///
    /// Negligible amount of space used by the CircularBuffer beside the array itself.
    pub fn new(size: usize) -> Self {
        let size = size;
        let type_size = core::mem::size_of::<T>();
        let vector_size = type_size.checked_mul(size).unwrap();
        let aligment = core::mem::align_of::<T>();

        let layout = core::alloc::Layout::from_size_align(vector_size, aligment).unwrap();
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };

        CircularBuffer {
            buffer: ptr.cast(),
            w: 0,
            r: 0,
            size,
            full: false,
        }
    }

    /// Returns the amount of elements in the CircularBuffer in O(1)
    pub fn len(&self) -> usize {
        if self.full {
            return self.size;
        }
        if self.w > self.r {
            self.w - self.r
        } else if self.w == self.r {
            0
        } else {
            self.size - self.r + self.w
        }
    }

    fn next_inc(&self, i: usize) -> usize {
        (i + 1) % self.size
    }

    fn w_inc(&mut self) {
        self.w = self.next_inc(self.w);
    }

    fn r_inc(&mut self) {
        self.r = self.next_inc(self.r);
    }

    fn r_inc_of(&mut self, n: usize) {
        self.r = (self.r + n) % self.size;
    }

    fn write(&mut self, value: T) {
        let w_index = self.w;
        self.w_inc();
        unsafe {
            self.buffer.add(w_index).write(value);
        }
    }

    fn read(&mut self) -> T {
        let r_index = self.r;
        self.r_inc();
        unsafe {
            let ptr = self.buffer.add(r_index);
            ptr.read()
        }
    }

    fn drop(&mut self) {
        unsafe {
            let ptr = self.buffer.offset(self.w.try_into().unwrap());
            core::ptr::drop_in_place(ptr);
        }
    }

    /// Push a new element into the CircularBuffer in O(1) does not do any allocation.
    ///
    /// If the CircularBuffer is full, the first element of the CircularBuffer is overwritten.
    pub fn push(&mut self, value: T) -> usize {
        if self.full {
            // pointer to w must first be free, and the overwritten
            self.drop();
            self.r_inc();
        }
        self.write(value);
        if self.w == self.r {
            self.full = true;
            0
        } else {
            self.size - self.len()
        }
    }

    /// Main method to read elements out of the CircularBuffer.
    ///
    /// The return vector get filled, with as many as possible elements from the CircularBuffer.
    ///
    /// The available elements in the CircularBuffer are the same returned by the method `len`. The elements
    /// that the vector can accepts are given by `return_vector.capacity() - return_vector.len()`
    ///
    /// The method avoids allocating memory.
    ///
    /// Hence if the vector is already full, no elements are pushed into the vector.
    ///
    /// If the CircularBuffer is empty, no elements are pushed into the vector.
    ///
    /// If the vector can accept more elements that are prensent in the CircularBuffer, the vector
    /// get filled as much as possible and the CircularBuffer will remain empty.
    ///
    /// If the vector cannot accept all the element in the CircularBuffer, the vector get filled
    /// while the CircularBuffer will be left with some elements inside.
    ///
    /// The operation runs in O(n) with `n` number of elements pushed into the vector.
    pub fn fill(&mut self, return_vector: &mut Vec<T>) -> usize {
        let mut i = 0;
        while return_vector.capacity() - return_vector.len() > 0 {
            match self.next() {
                Some(element) => {
                    return_vector.push(element);
                    i += 1;
                }
                None => return i,
            }
        }
        i
    }

    fn split_in_ranges(&self) -> (core::ops::Range<usize>, Option<core::ops::Range<usize>>) {
        if self.r < self.w {
            (self.r..self.w, None)
        } else if self.r == self.w {
            if self.full {
                (self.r..self.size, Some(0..self.w))
            } else {
                (self.r..self.r, None)
            }
        } else {
            (self.r..self.size, Some(0..self.w))
        }
    }

    fn fill_vector_from_split(&mut self, range: core::ops::Range<usize>, vec: &mut Vec<T>) -> usize {
        let sink_capacity = vec.capacity() - vec.len();
        if sink_capacity == 0 {
            return 0;
        }
        if range.len() == 0 {
            return 0;
        }
        let to_push = if range.len() <= sink_capacity {
            range
        } else {
            let mut r = range;
            r.end = r.start + sink_capacity;
            r
        };

        unsafe {
            let ptr = vec.as_mut_ptr().add(vec.len());
            core::ptr::copy_nonoverlapping(self.buffer.add(to_push.start), ptr, to_push.len());
            vec.set_len(vec.len() + to_push.len());
        }

        self.r_inc_of(to_push.len());
        self.full = false;
        return to_push.len();
    }

    /// The `_fast_fill` method is supposed to be a faster alternative to the `fill` one.
    /// However, benchmarks failed to show any difference in performance.
    /// If the benchmark showed any difference, it was the `_fast_fill` method being a little slower.
    ///
    /// The `_fast_fill` method is more complex that the `fill` method, so I suggest to rely on the
    /// simpler `fill`. However both methods passed the same properties tests, so they should be
    /// equally correct.
    ///
    /// The `_fast_fill` is implemented using raw pointer and memcopy. While the `fill` method
    /// pull elements using the iterator and simply push them to the back of the vector.
    pub fn _fast_fill(&mut self, return_vector: &mut Vec<T>) -> usize {
        if self.len() == 0 {
            return 0;
        }
        let sink_capacity = return_vector.capacity() - return_vector.len();
        if sink_capacity == 0 {
            return 0;
        }
        let mut total_pushed = 0;
        let (r1, r2) = self.split_in_ranges();
        total_pushed += self.fill_vector_from_split(r1, return_vector);
        if total_pushed == sink_capacity {
            return total_pushed;
        }
        if let Some(r2) = r2 {
            total_pushed += self.fill_vector_from_split(r2, return_vector)
        }
        total_pushed
    }
}

impl<T: Clone> Clone for CircularBuffer<T> {
    fn clone(&self) -> Self {
        let mut new = CircularBuffer::new(self.size);
        new.w = self.w;
        new.r = self.r;
        new.size = self.size; // useless
        new.full = self.full;

        let (r1, r2) = self.split_in_ranges();
        for i in r1 {
            unsafe {
                let r_ptr = self.buffer.add(i);
                let e0 = r_ptr.read();
                let e1 = e0.clone();
                core::mem::forget(e0);
                let w_buffer = new.buffer as *mut T;
                let w_ptr = w_buffer.add(i);
                w_ptr.write(e1);
            }
        }
        if let Some(r2) = r2 {
            for i in r2 {
                unsafe {
                    let r_ptr = self.buffer.add(i);
                    let e0 = r_ptr.read();
                    let e1 = e0.clone();
                    core::mem::forget(e0);
                    let w_buffer = new.buffer as *mut T;
                    let w_ptr = w_buffer.add(i);
                    w_ptr.write(e1);
                }
            }
        }

        new
    }
}

/// Create an iterator, elements from the iterator are consumed and are not present anymore in the
/// buffer.
impl<T> core::iter::Iterator for CircularBuffer<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.len() {
            0 => None,
            _ => {
                self.full = false;
                Some(self.read())
            }
        }
    }
    /// The size_hint is correct, it is not an hint but it is the correct value.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for CircularBuffer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.len() == 0 {
            return write!(f, "CircularBuffer(<empty>)");
        }
        write!(f, "CircularBuffer(")?;
        let mut fake_read = self.r;
        let read = fake_read.try_into().unwrap();
        let element = unsafe {
            let ptr = self.buffer.offset(read);
            ptr.read()
        };
        core::fmt::Debug::fmt(&element, f)?;
        core::mem::forget(element);
        fake_read = self.next_inc(fake_read);
        while fake_read != self.w {
            write!(f, ", ")?;
            let read = fake_read.try_into().unwrap();
            let element = unsafe {
                let ptr = self.buffer.offset(read);
                ptr.read()
            };
            core::fmt::Debug::fmt(&element, f)?;
            core::mem::forget(element);
            fake_read = self.next_inc(fake_read);
        }
        write!(
            f,
            ") w: {:?}, r: {:?}, size: {:?}, full: {:?}",
            self.w, self.r, self.size, self.full
        )
    }
}

impl<T: core::fmt::Display> core::fmt::Display for CircularBuffer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.len() == 0 {
            return write!(f, "CircularBuffer(<empty>)");
        }
        write!(f, "CircularBuffer(")?;
        let mut fake_read = self.r;
        let read = fake_read.try_into().unwrap();
        let element = unsafe {
            let ptr = self.buffer.offset(read);
            ptr.read()
        };
        core::fmt::Display::fmt(&element, f)?;
        core::mem::forget(element);
        fake_read = self.next_inc(fake_read);
        while fake_read != self.w {
            write!(f, ", ")?;
            let read = fake_read.try_into().unwrap();
            let element = unsafe {
                let ptr = self.buffer.offset(read);
                ptr.read()
            };
            core::fmt::Display::fmt(&element, f)?;
            core::mem::forget(element);
            fake_read = self.next_inc(fake_read);
        }
        write!(f, ")")
    }
}
