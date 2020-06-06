use ubip_buffer::BipBuffer;

use std::thread;
use std::time::Duration;

fn main() {
    static mut STORAGE: [i32;10] = [0; 10];
    {
        let mut bip_buffer = unsafe { BipBuffer::new(&mut STORAGE) };
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();
        
        let producer = thread::spawn(move || {
            let mut produced_values = 0;
            for i in 0..100 {
                let mut produce_result = writer.produce(&[i, i*2, i*3]);

                while produce_result.is_err() {
                    produce_result = writer.produce_with_split(&[i, i*2, i*3]);
                }
                thread::sleep(Duration::from_millis(1));
                produced_values += 3;
            }
            println!("Produced {} values", produced_values);
        });
        let mut num_values_sum = 0usize;
        for _i in 0..130 {
            let values = reader.values();
            if values.len() > 0 {
                println!("Read {} values", values.len());
            }
            thread::sleep(Duration::from_millis(4));
            num_values_sum += values.len();
            reader.consume(values.len()).unwrap();
        }
        println!("Read a total of {} values", num_values_sum);
        producer.join().unwrap();
    }
}