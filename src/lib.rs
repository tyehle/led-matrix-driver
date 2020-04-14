#![no_std]

use embedded_hal as hal;
use nb::block;
use void;

#[derive(Debug)]
pub enum TError<S, P> {
    Serial(S),
    Pin(P),
}

const NUM_ROWS: usize = 8;
const NUM_COLS: usize = 16;
const SPI_BYTES: usize = NUM_COLS / 8;

// type SPI = dyn embedded_hal::spi::FullDuplex<u8, Error=()>;

pub struct LEDArray {
    pub array: [[u8; NUM_COLS]; NUM_ROWS],
    // spi: SPI,
}

impl LEDArray {
    pub fn prepare_row(&self, r: usize, buf: &mut [[u8; SPI_BYTES]; 4]) {
        assert!(r < NUM_ROWS);
        let row = self.array[r];

        for layer in 0..buf.len() {
            let mut output = 0u16;
            for brightness in &row {
                // grab brightness mod 2^layer
                let tmp = brightness % (2 << layer);
                // left shift output
                output = output << 1;
                // add to output
                output += tmp as u16 >> layer;
            }

            // update buffer[layer]
            for byte_number in (0..SPI_BYTES).rev() {
                buf[layer][byte_number] = output as u8;
                output = output >> 8;
            }
        }
    }
}

pub fn timing<T, D>(timer: &mut T, base_delay: D) -> Result<(), void::Void>
where
    T: hal::timer::CountDown,
    T::Time: core::convert::From<D>,
    D: core::ops::Mul<Output = D>,
    i32: core::convert::Into<D>,
{
    timer.start(base_delay * (1 << 3).into());
    // do some other stuff
    (0..10_000).sum::<i32>();
    // wait for the timer to finish
    block!(timer.wait())
}

pub fn spi<S>(bus: &mut S) -> Result<(), S::Error>
where
    S: hal::spi::FullDuplex<u8>,
{
    for &data in &[7, 5, 1] {
        block!(bus.send(data))?;
    }
    Ok(())
}

pub fn pins<P>(a: &mut P, b: &mut P) -> Result<(), P::Error>
where
    P: hal::digital::v2::OutputPin,
{
    a.set_high()?;
    b.set_low()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_prepare_row() {
        let row = [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];

        let array = LEDArray { array: [row; 8] };

        let mut buf = [[0u8; 2]; 4];
        array.prepare_row(0, &mut buf);

        assert_eq!(buf[0], [0b10101010, 0b10101010]); // for 1s
        assert_eq!(buf[1], [0b11001100, 0b11001100]); // for 2s
        assert_eq!(buf[2], [0b11110000, 0b11110000]); // for 4s
        assert_eq!(buf[3], [0b11111111, 0b00000000]); // for 8s
    }
}
