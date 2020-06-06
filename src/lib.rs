#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(core::fmt::Debug)]
pub enum WriteError
{
    SplitRequired{
        first_size: usize
    },
    NoRoom
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
                buffer: self
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
    buffer: *mut BipBuffer<'a, T>
}

unsafe impl<'a, T> core::marker::Send for BipBufferWriter<'a, T> {}

impl<'a, T: Clone> BipBufferWriter<'a, T>
{
    fn _store_after_read_write(buffer: &mut BipBuffer<T>, data: &[T], write: usize) {
        let amount = data.len();
        let new_watermark = write + amount;
        buffer.buffer[write..new_watermark].clone_from_slice(data);
        buffer.watermark.store(new_watermark, Ordering::SeqCst);
        buffer.write.store(new_watermark, Ordering::SeqCst);
    }

    fn _watermark_store_at_start(buffer: &mut BipBuffer<T>, data: &[T], write: usize)
    {
        let amount = data.len();
        buffer.buffer[0..amount].clone_from_slice(data);

        buffer.watermark.store(write, Ordering::SeqCst);
        buffer.write.store(amount, Ordering::SeqCst);
    }

    fn _store_between_write_read(buffer: &mut BipBuffer<T>, data: &[T], write: usize)
    {
        let amount = data.len();
        buffer.buffer[write..write+amount].clone_from_slice(data);
        buffer.write.store(write+amount, Ordering::SeqCst);
    }

    pub fn produce(&mut self, data: &[T]) -> Result<(), WriteError> {
        let amount = data.len();
        let buffer = unsafe { &mut (*self.buffer) };
        let len = buffer.buffer.len();
        let write = buffer.write.load(Ordering::SeqCst);
        let read = buffer.read.load(Ordering::SeqCst);

        if write >= read {
            // There must either be room in [write, len)
            // or in [0, read-1)
            if len - write >= amount {
                Self::_store_after_read_write(buffer, data, write);
                return Ok(());
            }
            else if read > amount {
                // We do have room in [0, read-1)
                Self::_watermark_store_at_start(buffer, data, write);
                return Ok(());
            }
            
            if read < 2 {
                return Err(WriteError::NoRoom);
            }
            else {
                let amount_to_len = len-write;
                let available_before_read = read-1;
                if (amount_to_len + available_before_read) < amount {
                    return Err(WriteError::NoRoom)
                }

                return Err(WriteError::SplitRequired{
                    first_size: amount_to_len
                });
            }
        }
        else if (write + amount) < read {
            // read leads write, so write cannot touch watermark at this point!
            // but write+amount will still be less than read so we can fit the
            // the data anyhow.
            Self::_store_between_write_read(buffer, data, write);
            return Ok(());
        }

        // No room!
        return Err(WriteError::NoRoom);
    }

    pub fn produce_with_split(&mut self, data: &[T]) -> Result<(), WriteError> {
        match self.produce(data) {
            Err(WriteError::SplitRequired{
                first_size, ..
            }) => {
                self.produce(&data[0..first_size]).and_then(|_| {
                    self.produce(&data[first_size..])
                })
            },
            _whatever => _whatever
        }
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

            assert!(writer.produce(&[*x]).is_ok());
            let buf = reader.values();
            assert_eq!(buf.len(), 1);
            assert_eq!(buf[0], *x);

            reader.consume(1).unwrap();
            let buf = reader.values();
            assert_eq!(buf.len(), 0);
        }
    }

    #[test]
    fn cannot_produce_more_than_buffer_size() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut _reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
        assert!(writer.produce(&expected).is_ok());

        assert!(match writer.produce(&[1]) {
            Err(WriteError::NoRoom) => true,
            _ => false
        });
    }

    #[test]
    fn produce_at_start_after_consume() {
        let expected = [10, 20, 30, 40];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert!(writer.produce(&expected).is_ok());
        assert!(match writer.produce(&[1]) {
            Err(WriteError::NoRoom) => true,
            _ => false
        });
        assert_eq!(reader.values().len(), expected.len());

        // Need to consume 2 to leave room for one non-written value
        reader.consume(2).unwrap();
        assert!(writer.produce(&[1]).is_ok());
        assert_eq!(reader.values().len(), expected.len()-2);

        assert!(match writer.produce(&[1]) {
            Err(WriteError::NoRoom) => true,
            _ => false
        });

        reader.consume(expected.len()-2).unwrap();
        assert_eq!(reader.values().len(), 1);
        assert!(writer.produce(&expected[0..expected.len()-1]).is_ok());
        assert_eq!(reader.values().len(), expected.len());
    }

    #[test]
    fn require_split() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        writer.produce(&expected[0..4]).unwrap();
        reader.consume(3).unwrap();
        assert!(match writer.produce(&expected[0..3]) {
            Err(WriteError::SplitRequired{
                first_size
            }) => first_size == 1,
            _ => false
        });

        assert!(writer.produce_with_split(&expected[0..3]).is_ok());
    }

    #[test]
    fn insert_watermark() {
        let expected = [10, 20, 30, 40, 50];
        let mut buffer = expected;
        let mut bip_buffer = BipBuffer::new(&mut buffer);
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        assert!(writer.produce(&expected[0..4]).is_ok());
        reader.consume(4).unwrap();
        assert!(writer.produce(&expected[0..2]).is_ok());
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

        assert!(writer.produce(&expected).is_ok());
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