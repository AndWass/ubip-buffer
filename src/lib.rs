#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(core::fmt::Debug, PartialEq)]
pub enum CommitError
{
    NotEnoughPrepared {
        prepared: usize
    }
}

#[derive(core::fmt::Debug, PartialEq)]
pub enum PrepareError
{
    UncommitedData {
        amount: usize,
    },
    NoRoom {
        max_available: usize
    }
}

/// A [BipBuffer](https://ferrous-systems.com/blog/lock-free-ring-buffer/) with completely external
/// storage.
///
/// The user has complete control over the storage, if it's dynamically allocated or statically.
pub struct BipBuffer<'a, T>
{
    buffer: &'a mut [T],
    watermark: AtomicUsize,
    read: AtomicUsize,
    write: AtomicUsize,
    rw_taken: bool
}

impl<'a, T> BipBuffer<'a, T>
{
    pub const fn capacity(&self) -> usize {
        return self.buffer.len();
    }
}

impl<'a, T: Clone> BipBuffer<'a, T>
{
    pub fn new(buffer: &'a mut [T]) -> Self {
        BipBuffer {
            buffer: buffer,
            watermark: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            rw_taken: false
        }
    }

    pub fn take_reader_writer(&mut self) -> Option<(BipBufferReader<'a, T>, BipBufferWriter<'a, T>)> {
        if self.rw_taken {
            return None;
        }
        self.rw_taken = true;
        return Some(
            (BipBufferReader {
                buffer: self
            },
            BipBufferWriter {
                buffer: self,
                prepared: 0
            })
        );
    }
}

pub struct BipBufferReader<'a, T>
{
    buffer: *mut BipBuffer<'a, T>
}

unsafe impl<'a, T> core::marker::Send for BipBufferReader<'a, T> {}

impl<'a, T: Clone> BipBufferReader<'a, T>
{
    pub fn values(&self) -> &'a [T] {
        let buffer = unsafe { &(*self.buffer) };
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);

        if write >= read {
            let size = write-read;
            return &buffer.buffer[read..(read+size)];
        }
        else {
            // Read leads write, so we can return [read, watermark)
            let watermark = buffer.watermark.load(Ordering::SeqCst);
            if watermark > read {
                // We return [read, watermark)
                return &buffer.buffer[read..watermark];
            }
            // We return [0, write)
            return &buffer.buffer[0..write];
        }
    }

    pub fn consume(&mut self, amount: usize) -> Result<(), usize> {
        let buffer = unsafe { &mut (*self.buffer) };
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);
        if write >= read {
            // Write leads read, we can at most consume (write - read) number of elements
            if amount > (write-read) {
                return Err(write-read);
            }
            buffer.read.store(read+amount, Ordering::SeqCst);
            return Ok(());
        }
        else {
            // Read leads write, we can at most consume all data from read to watermark
            // plus any written data before read.
            let watermark = buffer.watermark.load(Ordering::SeqCst);
            let available_for_consumption = watermark - read + write;
            if amount > available_for_consumption{
                return Err(available_for_consumption);
            }
            // Handle wrapping at the watermark
            let new_read = (read + amount) % watermark;
            buffer.read.store(new_read, Ordering::SeqCst);
            return Ok(());
        }
    }
}

pub struct BipBufferWriter<'a, T>
{
    buffer: *mut BipBuffer<'a, T>,
    prepared: usize
}

unsafe impl<'a, T> core::marker::Send for BipBufferWriter<'a, T> {}

impl<'a, T: Clone> BipBufferWriter<'a, T>
{
    pub fn capacity(&self) -> usize {
        return unsafe { (*self.buffer).capacity() };
    }
    pub fn prepare(&mut self, amount: usize) -> Result<&mut [T], PrepareError> {
        if self.prepared > 0 {
            return Err(PrepareError::UncommitedData {
                amount: self.prepared
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
                buffer.watermark.store(write+amount, Ordering::SeqCst);
                return Ok(&mut buffer.buffer[write..write+amount]);
            }
            else if read > amount {
                // We have room at the start of the buffer,
                // insert a watermark return [0..amount]
                buffer.watermark.store(write, Ordering::SeqCst);
                self.prepared = amount;
                return Ok(&mut buffer.buffer[0..amount]);
            }

            return Err(PrepareError::NoRoom{
                max_available: amount_available_to_end.max(read.saturating_sub(1))
            });
        }
        else {
            // Read leads write, so the only chance is that we have enough
            // room in [write..read-1)
            // for read to lead write read will always be greater than 1
            let available = (read - 1).saturating_sub(write);
            if available < amount {
                return Err(PrepareError::NoRoom{
                    max_available: available
                });
            }
            self.prepared = amount;
            return Ok(&mut buffer.buffer[write..write+amount]);
        }
    }

    pub fn prepare_trailing(&mut self) -> Result<&mut [T], PrepareError> {
        if self.prepared > 0 {
            return Err(PrepareError::UncommitedData {
                amount: self.prepared
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
                    self.prepared = read-1;
                    return Ok(&mut buffer.buffer[0..read-1]);
                }
                return Err(PrepareError::NoRoom{max_available: 0});
            }
            self.prepared = amount;
            buffer.watermark.store(len, Ordering::SeqCst);
            return Ok(&mut buffer.buffer[write..len]);
        }
        else {
            let available = (read - 1).saturating_sub(write);
            self.prepared = available;
            return Ok(&mut buffer.buffer[write..write+available]);
        }
    }

    pub fn commit(&mut self, amount: usize) -> Result<usize, CommitError> {
        if self.prepared < amount {
            return Err(CommitError::NotEnoughPrepared{
                prepared: self.prepared
            });
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
            }
            else {
                write + amount
            }
        }
        else {
            // We can just adjust write to
            // write+amount
            write + amount
        };

        buffer.write.store(new_write, Ordering::SeqCst);

        return Ok(self.prepared);
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn singleton_reader_writer() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (_, _) = bip_buffer.take_reader_writer().unwrap();
        assert!(bip_buffer.take_reader_writer().is_none());
    }
    #[test]
    fn single_produce_consume() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
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
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut _reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        writer.prepare(expected.len()).unwrap();

        assert_eq!(match writer.prepare(1) {
            Err(PrepareError::UncommitedData{amount}) => amount,
            _ => 255
        }, expected.len());
    }

    #[test]
    fn prepare_wraps() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();


        assert!(writer.prepare(expected.len()).is_ok());
        assert!(writer.commit(expected.len()).is_ok());
        assert_eq!(reader.values().len(), expected.len());
        assert_eq!(writer.prepare(1).err().unwrap(), PrepareError::NoRoom{max_available: 0});

        // Need to consume 2 to leave room for one non-written value
        reader.consume(2).unwrap();
        let buf = writer.prepare(1).unwrap();
        assert_eq!(buf[0], expected[0]);
        writer.commit(1).unwrap();
        assert_eq!(reader.values().len(), expected.len()-2);

        reader.consume(expected.len()-2).unwrap();
        assert_eq!(reader.values().len(), 1);
        writer.prepare(expected.len()-1).unwrap();
        writer.commit(expected.len()-1).unwrap();
        assert_eq!(reader.values().len(), expected.len());
    }

    #[test]
    fn insert_watermark() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
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
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert_eq!(reader.values().len(), 0);

        assert!(writer.prepare(expected.len()).is_ok());
        writer.commit(expected.len()).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len());
        assert_eq!(*buf, expected);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len()-1);
        assert_eq!(*buf, expected[1..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len()-2);
        assert_eq!(*buf, expected[2..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), expected.len()-3);
        assert_eq!(*buf, expected[3..]);

        reader.consume(1).unwrap();
        let buf = reader.values();
        assert_eq!(buf.len(), 0);
    }
}