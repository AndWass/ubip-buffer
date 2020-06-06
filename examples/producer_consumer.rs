use ubip_buffer::BipBuffer;

use std::thread;
use std::time::Duration;

extern crate rand;

use distributions::Distribution;
use rand::{distributions, thread_rng};

fn main() {
    static mut STORAGE: [i32; 13] = [0; 13];
    {
        let mut bip_buffer = unsafe { BipBuffer::new(&mut STORAGE) };
        let (mut reader, mut writer) = bip_buffer.take_reader_writer().unwrap();

        let producer = thread::spawn(move || {
            let mut produced_values = 0;
            let mut i = 0;
            let mut produced_sum = 0;
            let dist = distributions::Uniform::new_inclusive(3, writer.capacity());
            let mut rng = thread_rng();
            while produced_values < 300 {
                let amount_produced = match writer.prepare(dist.sample(&mut rng)) {
                    Ok(x) => {
                        let x_len = x.len() as i32;
                        for v in x {
                            *v = i;
                            produced_sum += i;
                            i += x_len;
                        }
                        x_len as usize
                    }
                    _ => 0,
                };
                if amount_produced > 0 {
                    writer.commit(amount_produced).unwrap();
                    produced_values += amount_produced;
                }
                thread::sleep(Duration::from_millis(1));
            }
            println!("Produced {} values", produced_values);
            println!("Produced sum = {}", produced_sum);
        });
        let mut num_values_sum = 0usize;
        let mut consumed_sum = 0;
        for _i in 0..150 {
            let values = reader.values();
            if values.len() > 0 {
                println!("Read {} values", values.len());
            }
            for v in values {
                consumed_sum += *v;
            }
            thread::sleep(Duration::from_millis(3));
            num_values_sum += values.len();
            reader.consume(values.len()).unwrap();
        }
        println!("Read a total of {} values", num_values_sum);
        println!("Consumed sum = {}", consumed_sum);
        producer.join().unwrap();
    }
}
