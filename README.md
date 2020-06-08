# ubip_buffer

A SPSC [BipBuffer](https://ferrous-systems.com/blog/lock-free-ring-buffer/) with external storage.

The buffer is circular and is divided into a writable part and a readable part.
Before writing data the data is prepared, allowing the user to retrieve a slice of the
buffer of the requested size (among others).

Data that is ready for consumption is commited, allowing that data to be read and consumed.
after data is consumed it is again made available for writing.

## Example

```rust
let mut buffer = [0; 10];
let mut bip_buffer = ubip_buffer::BipBuffer::new(&mut buffer);
let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
let to_write = writer.prepare(3).unwrap().copy_from_slice(&[1,2,3]);
writer.commit(3).unwrap();
assert_eq!(reader.values(), [1,2,3]);
// reader.values() will always start with [1, 2, 3] until some values are consumed
reader.consume(1);
assert_eq!(reader.values(), [2,3]);
// Commited values may appear in calls to values()
writer.prepare(1).unwrap()[0] = 4;
writer.commit(1);
assert_eq!(reader.values(), [2,3,4]);
```

License: BSL-1.0
