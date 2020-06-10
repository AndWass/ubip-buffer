//          Copyright Andreas Wass 2004 - 2020.
// Distributed under the Boost Software License, Version 1.0.
//    (See accompanying file LICENSE or copy at
//          https://www.boost.org/LICENSE_1_0.txt)

#![no_std]

//! A SPSC [BipBuffer](https://ferrous-systems.com/blog/lock-free-ring-buffer/) with external storage.
//!
//! The buffer is circular and is divided into a writable part and a readable part.
//! Before writing data the data is prepared, allowing the user to retrieve a slice of the
//! buffer of the requested size (among others).
//!
//! Data that is ready for consumption is commited, allowing that data to be read and consumed.
//! after data is consumed it is again made available for writing.
//!
//! # Example
//!
//! ```
//! # use ubip_buffer::*;
//! let mut buffer = [0; 10];
//! let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
//! let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
//! let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
//! writer.commit(3).unwrap();
//! assert_eq!(reader.values(), [1,2,3]);
//! // reader.values() will always start with [1, 2, 3] until some values are consumed
//! reader.consume(1);
//! assert_eq!(reader.values(), [2,3]);
//! // Commited values may appear in calls to values()
//! writer.prepare(1).unwrap()[0] = 4;
//! writer.commit(1);
//! assert_eq!(reader.values(), [2,3,4]);
//! ```

use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(core::fmt::Debug, PartialEq)]
pub enum CommitError {
    NotEnoughPrepared { prepared: usize },
}

#[derive(core::fmt::Debug, PartialEq)]
pub enum PrepareError {
    UncommitedData { amount: usize },
    NoRoom { max_available: usize },
}

/// Trait used by BipBuffers to reference storage.
///
/// This trait can be implemented by custom types to provide
/// custom storage schemas.
///
/// # Example
///
/// ```
/// # use ubip_buffer::*;
/// struct MyCustomStorage {
///     storage: [i32; 32]
/// }
/// impl Storage for MyCustomStorage {
///     type ValueType = i32;
///     fn slice(&self, range: core::ops::Range<usize>) -> &[i32] {
///         &self.storage[range.start..range.end]
///     }
///     fn mut_slice(&mut self, range: core::ops::Range<usize>) -> &mut [i32] {
///         &mut self.storage[range.start..range.end]
///     }
///     fn len(&self) -> usize { self.storage.len() }
/// }
/// let mut bip_buffer = BipBuffer::new(MyCustomStorage { storage: [0; 32] });
/// let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
/// let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
/// writer.commit(3).unwrap();
/// assert_eq!(reader.values(), [1,2,3]);
/// // reader.values() will always start with [1, 2, 3] until some values are consumed
/// reader.consume(1);
/// assert_eq!(reader.values(), [2,3]);
/// // Commited values may appear in calls to values()
/// writer.prepare(1).unwrap()[0] = 4;
/// writer.commit(1);
/// assert_eq!(reader.values(), [2,3,4]);
/// ```
pub trait Storage {
    type ValueType;
    /// Provides a slice over the specified range.
    fn slice(&self, range: core::ops::Range<usize>) -> &[Self::ValueType];
    /// Provides a mutable slice over the specified range.
    fn mut_slice(&mut self, range: core::ops::Range<usize>) -> &mut [Self::ValueType];
    /// The total lenght of the storage.
    fn len(&self) -> usize;
}

pub struct StorageRef<'a, T>
where
    T: Clone,
{
    buffer: &'a mut [T],
}

impl<'a, T> StorageRef<'a, T>
where
    T: Clone,
{
    pub fn new(buffer: &'a mut [T]) -> Self {
        Self { buffer }
    }
}

impl<'a, T> Storage for StorageRef<'a, T>
where
    T: Clone,
{
    type ValueType = T;
    fn slice(&self, range: core::ops::Range<usize>) -> &[T] {
        &self.buffer[range.start..range.end]
    }
    fn mut_slice(&mut self, range: core::ops::Range<usize>) -> &mut [T] {
        &mut self.buffer[range.start..range.end]
    }
    fn len(&self) -> usize {
        self.buffer.len()
    }
}

pub struct UnsafeStorageRef<T>
where
    T: Clone,
{
    buffer: *mut T,
    len: usize,
}

impl<'a, T> UnsafeStorageRef<T>
where
    T: Clone,
{
    pub fn new(buffer: &mut [T]) -> Self {
        Self {
            buffer: buffer.as_mut_ptr(),
            len: buffer.len(),
        }
    }
}

impl<T> Storage for UnsafeStorageRef<T>
where
    T: Clone,
{
    type ValueType = T;
    fn slice(&self, range: core::ops::Range<usize>) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.buffer.add(range.start), range.len()) }
    }
    fn mut_slice(&mut self, range: core::ops::Range<usize>) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.buffer.add(range.start), range.len()) }
    }
    fn len(&self) -> usize {
        self.len
    }
}

/// A SPSC [BipBuffer](https://ferrous-systems.com/blog/lock-free-ring-buffer/) with external storage.
///
/// The BipBuffer itself only provides access to a reader and a writer handle, which provides
/// the actual access.
///
/// The BipBuffer object must be kept alive for as long as a reader or writer
/// is used.
///
/// Both readers and writers implement `Send` so they can be sent to a secondary thread.
///
/// # Requirements
///
/// * `T` must implement `Clone`.
pub struct BipBuffer<St>
where
    St: Storage,
{
    buffer: St,
    watermark: AtomicUsize,
    read: AtomicUsize,
    write: AtomicUsize,
    rw_taken: bool,
}

impl<St> BipBuffer<St>
where
    St: Storage,
{
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }
}

impl<St> BipBuffer<St>
where
    St: Storage,
{
    /// Construct a new BipBuffer from an external buffer
    ///
    /// # Arguments
    ///
    /// * `buffer` - The backing storage for the buffer.
    pub fn new(buffer: St) -> Self {
        BipBuffer {
            buffer,
            watermark: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            rw_taken: false,
        }
    }

    /// Take readers and writers from the buffer
    ///
    /// This is a one-shot function, any subsequent calls to this function
    /// will return `None`.
    pub fn take_reader_writer(&mut self) -> Option<(BipBufferReader<St>, BipBufferWriter<St>)> {
        if self.rw_taken {
            return None;
        }
        self.rw_taken = true;
        Some((
            BipBufferReader { buffer: self },
            BipBufferWriter {
                buffer: self,
                prepared: 0,
            },
        ))
    }
}

/// The reader handle into a `BipBuffer`
///
/// Values are read and consumed, making parts of the buffer
/// available for writing again.
///
/// Values are fetched using the `values()` function.
/// Unconsumed values will always be available in any
/// second calls to `values()`.
///
/// # Example
/// ```
/// # use ubip_buffer::*;
/// let mut buffer = [0; 10];
/// let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
/// let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
/// let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
/// writer.commit(3).unwrap();
/// assert_eq!(reader.values(), [1,2,3]);
/// // reader.values() will always start with [1, 2, 3] until some values are consumed
/// reader.consume(1);
/// assert_eq!(reader.values(), [2,3]);
/// // Commited values may appear in calls to values()
/// writer.prepare(1).unwrap()[0] = 4;
/// writer.commit(1);
/// assert_eq!(reader.values(), [2,3,4]);
/// ```
pub struct BipBufferReader<St>
where
    St: Storage,
{
    buffer: *mut BipBuffer<St>,
}

unsafe impl<St> core::marker::Send for BipBufferReader<St> where St: Storage {}

impl<St> BipBufferReader<St>
where
    St: Storage,
{
    /// Gets a reference to a committed slice of data.
    ///
    /// It might be necessary to read values and consume them
    /// multiple times to read all values in the underlying
    /// buffer.
    ///
    /// Call to `self.consume()` to ensure that new values are read.
    pub fn values(&self) -> &[St::ValueType] {
        let buffer = unsafe { &(*self.buffer) };
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);

        if write >= read {
            let size = write - read;
            buffer.buffer.slice(read..(read + size))
        } else {
            // Read leads write, so we can return [read, watermark)
            let watermark = buffer.watermark.load(Ordering::SeqCst);
            if watermark > read {
                // We return [read, watermark)
                return &buffer.buffer.slice(read..watermark);
            }
            // We return [0, write)
            buffer.buffer.slice(0..write)
        }
    }

    /// Consume processed values.
    ///
    /// Consuming values frees up buffer space for writing new values,
    /// and ensures that any consumed values are removed from the start
    /// of the slice returned by `values()`
    ///
    /// Only consume up to the last known amount of values returned from `values()`
    ///
    /// # Arguments
    ///
    /// * `amount` - The number of values to consume.
    ///
    /// # Returns
    ///
    /// `Ok(())` if `amount` values were consumed, otherwise the maximum number of values that could be
    /// consumed when this function was called.
    pub fn consume(&mut self, amount: usize) -> Result<(), usize> {
        let buffer = unsafe { &mut (*self.buffer) };
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);
        if write >= read {
            // Write leads read, we can at most consume (write - read) number of elements
            if amount > (write - read) {
                return Err(write - read);
            }
            buffer.read.store(read + amount, Ordering::SeqCst);
            Ok(())
        } else {
            // Read leads write, we can at most consume all data from read to watermark
            // plus any written data before read.
            let watermark = buffer.watermark.load(Ordering::SeqCst);
            let available_for_consumption = watermark - read + write;
            if amount > available_for_consumption {
                return Err(available_for_consumption);
            }
            // Handle wrapping at the watermark
            let new_read = (read + amount) % watermark;
            buffer.read.store(new_read, Ordering::SeqCst);
            Ok(())
        }
    }
}

/// The writer handle into a `BipBuffer`
///
/// A write operation is done in two parts:
///
/// 1. Prepare a buffer area for writing. A successful prepare
/// returns a mutable slice to the internal buffer.
///
/// 2. Commiting of finished data. This allows the data to be written.
///
/// A prepared area can be commited in parts or completely in one go, but
/// must be commited from the start of the prepared area.
///
/// ```
/// # use ubip_buffer::*;
/// let mut buffer = [0; 10];
/// let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
/// let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
/// let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
/// assert_eq!(reader.values(), []);
/// writer.commit(1).unwrap();
/// assert_eq!(reader.values(), [1]);
/// writer.commit(1).unwrap();
/// assert_eq!(reader.values(), [1, 2]);
/// writer.commit(1).unwrap();
/// assert_eq!(reader.values(), [1, 2, 3]);
/// ```
pub struct BipBufferWriter<St>
where
    St: Storage,
{
    buffer: *mut BipBuffer<St>,
    prepared: usize,
}

impl<St> BipBufferWriter<St>
where
    St: Storage,
{
    /// Returns the total capacity of the `BipBuffer`
    ///
    /// This is usually not that useful, depending on where the read pointer is
    /// located only parts of the total capacity is availabe.
    pub fn capacity(&self) -> usize {
        unsafe { (*self.buffer).capacity() }
    }
}

unsafe impl<St> core::marker::Send for BipBufferWriter<St> where St: Storage {}

impl<St> BipBufferWriter<St>
where
    St: Storage,
{
    /// Try to prepare a set amount of data for commitment.
    ///
    /// Depending on the current state of the `BipBuffer`, data can
    /// either be sliced from `[write, len)`, `[0, read-1)` or `[write, read-1)`.
    ///
    /// # Arguments
    ///
    /// * `amount` - The number of elements requested.
    ///
    /// # Returns
    ///
    /// ## Success
    ///
    /// A slice to the internal buffer with `len()` equal to `amount`. This should be written to
    /// and commited after the buffer is done.
    pub fn prepare(&mut self, amount: usize) -> Result<&mut [St::ValueType], PrepareError> {
        if self.prepared > 0 {
            return Err(PrepareError::UncommitedData {
                amount: self.prepared,
            });
        }
        let buffer = unsafe { &mut (*self.buffer) };
        let len = buffer.buffer.len();
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);
        if write >= read {
            // write leads read, we can either prepare an area at [write, len)
            // or at [0, read-1)
            let amount_available_to_end = len - write;
            if amount_available_to_end >= amount {
                self.prepared = amount;
                buffer.watermark.store(write + amount, Ordering::SeqCst);
                return Ok(buffer.buffer.mut_slice(write..(write + amount)));
            } else if read > amount {
                // We have room at the start of the buffer,
                // insert a watermark return [0..amount]
                buffer.watermark.store(write, Ordering::SeqCst);
                self.prepared = amount;
                return Ok(buffer.buffer.mut_slice(0..amount));
            }

            return Err(PrepareError::NoRoom {
                max_available: amount_available_to_end.max(read.saturating_sub(1)),
            });
        } else {
            // Read leads write, so the only chance is that we have enough
            // room in [write..read-1)
            // for read to lead write read will always be greater than 1
            let available = (read - 1).saturating_sub(write);
            if available < amount {
                return Err(PrepareError::NoRoom {
                    max_available: available,
                });
            }
            self.prepared = amount;
            return Ok(buffer.buffer.mut_slice(write..write + amount));
        }
    }

    /// Prepares and returns any part of the buffer trailing the write pointer.
    ///
    /// If the layout of the BipBuffer is
    /// ```ignored
    /// |--read---write------len|
    /// ```
    /// then the slice `[write, len)` will be prepared and returned.
    ///
    /// If the layout of the BipBuffer is
    /// ```ignored
    /// |--write----read------len|
    /// ```
    /// then the slice `[write, read-1)` will be returned (given that `read-1-write > 1` holds)
    ///
    /// If `write == len` and `read >= 2` then the slice `[0, read-1)`
    /// will be returned.
    pub fn prepare_trailing(&mut self) -> Result<&mut [St::ValueType], PrepareError> {
        if self.prepared > 0 {
            return Err(PrepareError::UncommitedData {
                amount: self.prepared,
            });
        }

        let buffer = unsafe { &mut (*self.buffer) };
        let len = buffer.buffer.len();
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);
        if write >= read {
            // We want to prepare a buffer going from [write..len)
            let amount = len - write;
            if amount == 0 {
                // write == len, check if we have room at the beginning of the buffer
                if read >= 2 {
                    buffer.watermark.store(len, Ordering::SeqCst);
                    self.prepared = read - 1;
                    return Ok(buffer.buffer.mut_slice(0..read - 1));
                }
                return Err(PrepareError::NoRoom { max_available: 0 });
            }
            self.prepared = amount;
            buffer.watermark.store(len, Ordering::SeqCst);
            Ok(buffer.buffer.mut_slice(write..len))
        } else {
            let available = (read - 1).saturating_sub(write);
            if available == 0 {
                return Err(PrepareError::NoRoom { max_available: 0 });
            }
            self.prepared = available;
            Ok(buffer.buffer.mut_slice(write..write + available))
        }
    }

    /// Prepares and returns the biggest slice of continues data available.
    ///
    /// Depending on the state of the buffer this is either located
    /// at `[write, len)`, `[0, read-1)` or `[write, read-1)`
    pub fn prepare_max(&mut self) -> Result<&mut [St::ValueType], PrepareError> {
        if self.prepared > 0 {
            return Err(PrepareError::UncommitedData {
                amount: self.prepared,
            });
        }

        let buffer = unsafe { &mut (*self.buffer) };
        let len = buffer.buffer.len();
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);

        if write >= read {
            let available_before_read = read.saturating_sub(1);
            let available_before_len = len - write;
            if available_before_len >= available_before_read {
                if available_before_len > 0 {
                    buffer.watermark.store(len, Ordering::SeqCst);
                    self.prepared = available_before_len;
                    return Ok(buffer.buffer.mut_slice(write..len));
                }
                return Err(PrepareError::NoRoom { max_available: 0 });
            } else {
                buffer.watermark.store(write, Ordering::SeqCst);
                self.prepared = available_before_read;
                return Ok(buffer.buffer.mut_slice(0..available_before_read));
            }
        } else {
            let available = (read - 1).saturating_sub(write);
            if available >= 1 {
                self.prepared = available;
                return Ok(buffer.buffer.mut_slice(write..write + available));
            }
        }

        Err(PrepareError::NoRoom { max_available: 0 })
    }

    /// Commits a previsouly prepared part of data.
    ///
    /// Commiting data immediately makes the data available for reading.
    ///
    /// All data must be commited before new data can be prepared.
    ///
    /// # Arguments
    ///
    /// * `amount` - The number of elements to commit.
    pub fn commit(&mut self, amount: usize) -> Result<usize, CommitError> {
        if self.prepared < amount {
            return Err(CommitError::NotEnoughPrepared {
                prepared: self.prepared,
            });
        }

        if amount == 0 {
            return Ok(self.prepared);
        }

        let buffer = unsafe { &mut (*self.buffer) };
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);

        self.prepared -= amount;

        let new_write = if write >= read {
            // We need to ajdust write to
            // write + amount % watermark
            let watermark = buffer.watermark.load(Ordering::SeqCst);
            if watermark == write {
                amount
            } else {
                write + amount
            }
        } else {
            // We can just adjust write to
            // write+amount
            write + amount
        };

        buffer.write.store(new_write, Ordering::SeqCst);

        Ok(self.prepared)
    }

    /// Discards uncommited but prepared data.
    ///
    /// After discard it is safe to prepare a new area for writing.
    ///
    /// # Returns
    ///
    /// The number of elements that were discarded.
    ///
    /// # Example
    ///
    /// ```
    /// # use ubip_buffer::*;
    /// let mut buffer = [0; 10];
    /// let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
    /// let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
    /// let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
    /// assert_eq!(reader.values(), []);
    /// writer.commit(1).unwrap();
    /// assert_eq!(reader.values(), [1]);
    /// assert!(writer.prepare(1).is_err());
    /// assert_eq!(writer.discard(), 2);
    /// assert_eq!(reader.values(), [1]);
    /// let to_write = writer.prepare(2).unwrap();
    /// // This was written but discarded previously
    /// assert_eq!(to_write[0], 2);
    /// ```
    pub fn discard(&mut self) -> usize {
        let retval = self.prepared;
        self.prepared = 0;
        return retval;
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn singleton_reader_writer() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (_, _) = bip_buffer.take_reader_writer().unwrap();
        assert!(bip_buffer.take_reader_writer().is_none());
    }
    #[test]
    fn single_produce_consume() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        for x in &expected {
            assert_eq!(reader.values().len(), 0);

            let prepared = writer.prepare(1).unwrap();
            prepared[0] = *x;
            assert_eq!(reader.values().len(), 0);
            assert!(writer.commit(1).is_ok());

            let buf = reader.values();
            assert_eq!(buf.len(), 1);
            assert_eq!(buf[0], *x);

            reader.consume(1).unwrap();
            let buf = reader.values();
            assert_eq!(buf.len(), 0);
        }
    }

    #[test]
    fn cannot_prepare_more_than_once() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut _reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        writer.prepare(expected.len()).unwrap();

        assert_eq!(
            match writer.prepare(1) {
                Err(PrepareError::UncommitedData { amount }) => amount,
                _ => 255,
            },
            expected.len()
        );
    }

    #[test]
    fn prepare_wraps() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert!(writer.prepare(expected.len()).is_ok());
        assert!(writer.commit(expected.len()).is_ok());
        assert_eq!(reader.values().len(), expected.len());
        assert_eq!(
            writer.prepare(1).err().unwrap(),
            PrepareError::NoRoom { max_available: 0 }
        );

        // Need to consume 2 to leave room for one non-written value
        reader.consume(2).unwrap();
        let buf = writer.prepare(1).unwrap();
        assert_eq!(buf[0], expected[0]);
        writer.commit(1).unwrap();
        assert_eq!(reader.values().len(), expected.len() - 2);

        reader.consume(expected.len() - 2).unwrap();
        assert_eq!(reader.values().len(), 1);
        writer.prepare(expected.len() - 1).unwrap();
        writer.commit(expected.len() - 1).unwrap();
        assert_eq!(reader.values().len(), expected.len());
    }

    #[test]
    fn insert_watermark() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert!(writer.prepare(4).is_ok());
        writer.commit(4).unwrap();
        reader.consume(4).unwrap();
        assert!(writer.prepare(2).is_ok());
        writer.commit(2).unwrap();
        assert_eq!(reader.values().len(), 2);
        assert_eq!(*reader.values(), expected[0..2]);
    }

    #[test]
    fn single_produce_multi_consume() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert_eq!(reader.values().len(), 0);

        assert!(writer.prepare(expected.len()).is_ok());
        writer.commit(expected.len()).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len());
        assert_eq!(*buf, expected);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len() - 1);
        assert_eq!(*buf, expected[1..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len() - 2);
        assert_eq!(*buf, expected[2..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len() - 3);
        assert_eq!(*buf, expected[3..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn prepare_max() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        let expect_all = writer.prepare_max().unwrap().len();
        assert_eq!(expect_all, expected.len());
        writer.commit(expect_all).unwrap();
        reader.consume(expect_all).unwrap();

        let expect_before_read = writer.prepare_max().unwrap().len();
        assert_eq!(expect_before_read, writer.capacity() - 1);
        writer.commit(expect_before_read).unwrap();
        reader.consume(expect_before_read).unwrap();

        let prepared = writer.prepare_trailing().unwrap().len();
        writer.commit(prepared).unwrap();
        reader.consume(prepared).unwrap();

        // Prepare 4 elements to move write to index_of(40), then consume 4 elements
        // this moves read to index_of(40) as well, then prepare max which should set watermark to index_of(40)
        // and give [0..3]
        writer.prepare(4).unwrap();
        writer.commit(4).unwrap();
        reader.consume(4).unwrap();

        let prepared = writer.prepare_max().unwrap();
        let prepared_len = prepared.len();
        assert_eq!(prepared.len(), 3);
        assert_eq!(prepared[0], 10);
        writer.commit(prepared_len).unwrap();

        let read = reader.values();
        assert_eq!(read.len(), 3);
        assert_eq!(read[0], 10);
        reader.consume(3).unwrap();

        let rest = writer.prepare_trailing().unwrap().len();
        writer.commit(rest).unwrap();
        let rest = writer.prepare(1).unwrap();
        let rest_len = rest.len();
        assert_eq!(rest[0], 10);
        writer.commit(rest_len).unwrap();

        assert_eq!(reader.values()[0], 40);
        assert_eq!(reader.values().len(), 2);

        let rest = writer.prepare_max().unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0], 20);
        writer.commit(1).unwrap();
    }

    #[test]
    fn discard_write_resets_writer() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(StorageRef::new(&mut buffer));
        let (reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert_eq!(reader.values(), []);
        let prepared = writer.prepare(2).unwrap();
        assert_eq!(prepared[0], 10);
        prepared[0] = 1;
        writer.commit(1).unwrap();
        assert_eq!(writer.discard(), 1);
    }
}
