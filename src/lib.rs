#![no_std]

use embedded_hal as hal;
use hal::digital::v2::OutputPin;
use hal::spi::FullDuplex;
use nb::block;
use void;

const ROW_BITS: usize = 3;
const COL_BITS: usize = 4;
const LAYER_BITS: usize = 4;

const NUM_ROWS: usize = 1 << ROW_BITS;
const NUM_COLS: usize = 1 << COL_BITS;
const SPI_BYTES: usize = NUM_COLS / 8;

pub struct LEDArray<Pin, Timer, SPI> {
    pub array: [[u8; NUM_COLS]; NUM_ROWS],

    pub row_pins: [Pin; ROW_BITS],

    pub timer: Timer,

    pub spi: SPI,
    pub reg_pin: Pin,
    pub output_disable: Pin,
}

pub enum LEDError<P, S> {
    PinError(P),
    SPIError(S),
}

impl<Pin, Timer, SPI> LEDArray<Pin, Timer, SPI> {
    pub fn write_layer(&mut self, layer: &[u8], row: Option<usize>) -> Result<(), LEDError<Pin::Error, SPI::Error>>
    where
        Pin: OutputPin,
        Timer: hal::timer::CountDown,
        SPI: FullDuplex<u8>,
    {
        // prepare to latch the shift registers
        self.reg_pin.set_low().map_err(LEDError::PinError)?;

        // write the shift register data
        for &data in layer {
            block!(self.spi.send(data)).map_err(LEDError::SPIError)?;
        }

        // wait for the previous layer's time to end
        block!(self.timer.wait()).unwrap(); // Err is Void

        match row {
            // we aren't changing rows, so just latch the shift registers
            None => self.reg_pin.set_high().map_err(LEDError::PinError)?,

            // we are switching rows
            Some(row) => {
                // disabel the columns while we are writing to the row pins
                self.output_disable.set_high().map_err(LEDError::PinError)?;
                // update the row pins
                for (i, row_pin) in self.row_pins.iter_mut().enumerate() {
                    if ((row >> i) & 1) == 1 {
                        row_pin.set_high().map_err(LEDError::PinError)?;
                    } else {
                        row_pin.set_low().map_err(LEDError::PinError)?;
                    }
                }

                // latch the shift registers
                self.reg_pin.set_high().map_err(LEDError::PinError)?;

                // enable the correct row
                self.output_disable.set_low().map_err(LEDError::PinError)?;
            }
        }

        Ok(())
    }

    pub fn scan<T>(&mut self, base_time: T) -> Result<(), LEDError<Pin::Error, SPI::Error>>
    where
        Pin: OutputPin,
        Timer: hal::timer::CountDown,
        T: Into<Timer::Time> + Copy,
        SPI: FullDuplex<u8>,
    {
        let mut layers = [[0u8; SPI_BYTES]; LAYER_BITS];

        for row in 0..NUM_ROWS {
            self.prepare_row(row, &mut layers);

            for layer in 0..LAYER_BITS {
                self.write_layer(&layers[layer], if layer == 0 {Some(row)} else {None})?;

                // set the timer for this layer
                let real_delay = base_time; // TODO: Change the delay based on the layer number
                self.timer.start(real_delay);
            }
        }

        Ok(())
    }

    pub fn prepare_row(&self, r: usize, buf: &mut [[u8; SPI_BYTES]; LAYER_BITS]) {
        assert!(r < NUM_ROWS); // Maybe return an error somehow?
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
mod tests {
    use super::*;
    mod mock;
    use mock::*;

    fn mock_array() -> LEDArray<MockPin, MockTimer, MockSPI> {
        LEDArray {
            array: [[0; 16]; 8],

            row_pins: [MockPin { state: false }; 3],

            timer: MockTimer { tries: 0 },

            spi: MockSPI { written: heapless::Vec::new() },
            reg_pin: MockPin { state: false },
            output_disable: MockPin { state: false },
        }
    }

    #[test]
    fn test_prepare_row() {
        let row = [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];

        let mut array = mock_array();
        array.array = [row; 8];

        let mut buf = [[0u8; 2]; 4];
        array.prepare_row(0, &mut buf);

        assert_eq!(buf[0], [0b10101010, 0b10101010]); // for 1s
        assert_eq!(buf[1], [0b11001100, 0b11001100]); // for 2s
        assert_eq!(buf[2], [0b11110000, 0b11110000]); // for 4s
        assert_eq!(buf[3], [0b11111111, 0b00000000]); // for 8s
    }

    #[test]
    fn test_write_layer() {
        let mut array = mock_array();

        array.timer.tries = 6;
        array.write_layer(&[83, 106], None).unwrap_or(());
        assert_eq!(array.spi.written, [83, 106]);
        assert_eq!(array.timer.tries, 0);

        array.write_layer(&[13], Some(3)).unwrap_or(());
        assert_eq!(array.row_pins[2].state, false);
        assert_eq!(array.row_pins[1].state, true);
        assert_eq!(array.row_pins[0].state, true);
    }
}
