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

pub struct LEDArray<R0, R1, R2, Timer, SPI, Reg, OD> {
    pub array: [[u8; NUM_COLS]; NUM_ROWS],

    pub row_pins: (R0, R1, R2),

    pub timer: Timer,

    pub spi: SPI,
    pub reg_pin: Reg,
    pub output_disable: OD,
}

pub enum LEDError<P, S> {
    PinError(P),
    SPIError(S),
}

impl<R0, R1, R2, Timer, SPI, Reg, OD> LEDArray<R0, R1, R2, Timer, SPI, Reg, OD> {
    fn write_row<PinError>(&mut self, row: usize) -> Result<(), PinError>
    where
        R0: OutputPin<Error = PinError>,
        R1: OutputPin<Error = PinError>,
        R2: OutputPin<Error = PinError>,
    {
        #[inline]
        fn set_pin<P>(pin: &mut P, value: bool) -> Result<(), P::Error>
        where
            P: OutputPin,
        {
            if value {
                pin.set_high()
            } else {
                pin.set_low()
            }
        }

        set_pin(&mut self.row_pins.0, ((row >> 0) & 1) == 1)?;
        set_pin(&mut self.row_pins.1, ((row >> 1) & 1) == 1)?;
        set_pin(&mut self.row_pins.2, ((row >> 2) & 1) == 1)?;
        Ok(())
    }

    pub fn write_layer<PinError>(
        &mut self,
        layer: &[u8],
        row: Option<usize>,
    ) -> Result<(), LEDError<PinError, SPI::Error>>
    where
        R0: OutputPin<Error = PinError>,
        R1: OutputPin<Error = PinError>,
        R2: OutputPin<Error = PinError>,
        Timer: hal::timer::CountDown,
        SPI: FullDuplex<u8>,
        Reg: OutputPin<Error = PinError>,
        OD: OutputPin<Error = PinError>,
    {
        // prepare to latch the shift registers
        self.reg_pin.set_low().map_err(LEDError::PinError)?;

        // write the shift register data
        for &data in layer {
            block!(self.spi.send(!data)).map_err(LEDError::SPIError)?;
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
                self.write_row(row).map_err(LEDError::PinError)?;

                // latch the shift registers
                self.reg_pin.set_high().map_err(LEDError::PinError)?;

                // enable the correct row
                self.output_disable.set_low().map_err(LEDError::PinError)?;
            }
        };

        Ok(())
    }

    pub fn scan<T, PinError>(&mut self, base_freq: T) -> Result<(), LEDError<PinError, SPI::Error>>
    where
        R0: OutputPin<Error = PinError>,
        R1: OutputPin<Error = PinError>,
        R2: OutputPin<Error = PinError>,
        Timer: hal::timer::CountDown,
        T: Into<Timer::Time> + Copy + core::ops::Shl<usize, Output = T>,
        SPI: FullDuplex<u8>,
        Reg: OutputPin<Error = PinError>,
        OD: OutputPin<Error = PinError>,
    {
        let mut layers = [[0u8; SPI_BYTES]; LAYER_BITS];

        for row in 0..NUM_ROWS {
            self.prepare_row(row, &mut layers);

            for layer in 0..LAYER_BITS {
                self.write_layer(&layers[layer], if layer == 0 { Some(row) } else { None })?;

                // set the timer for this layer
                let freq = base_freq << (LAYER_BITS - layer - 1);
                self.timer.start(freq);
            }
        }

        Ok(())
    }

    pub fn prepare_row(&self, r: usize, buf: &mut [[u8; SPI_BYTES]; LAYER_BITS]) {
        assert!(r < NUM_ROWS); // Maybe return an error somehow?
        let row = self.array[r];

        for layer in 0..buf.len() {
            let mut output = 0u16;
            for brightness in row.iter().rev() {
                // grab brightness mod 2^layer
                let tmp = brightness % (2 << layer);
                // left shift output
                output = output << 1;
                // add to output
                output += tmp as u16 >> layer;
            }

            // update buffer[layer]
            buf[layer] = output.to_be_bytes();
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

    fn mock_array() -> LEDArray<MockPin, MockPin, MockPin, MockTimer, MockSPI, MockPin, MockPin> {
        LEDArray {
            array: [[0; 16]; 8],

            row_pins: (MockPin::new(), MockPin::new(), MockPin::new()),

            timer: MockTimer { tries: 0 },

            spi: MockSPI {
                written: heapless::Vec::new(),
            },
            reg_pin: MockPin::new(),
            output_disable: MockPin::new(),
        }
    }

    #[test]
    fn test_prepare_row() {
        let row = [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];

        let mut array = mock_array();
        array.array = [row; 8];

        let mut buf = [[0u8; 2]; 4];
        array.prepare_row(0, &mut buf);

        assert_eq!(buf[0], [0b01010101, 0b01010101]); // for 1s
        assert_eq!(buf[1], [0b00110011, 0b00110011]); // for 2s
        assert_eq!(buf[2], [0b00001111, 0b00001111]); // for 4s
        assert_eq!(buf[3], [0b00000000, 0b11111111]); // for 8s
    }

    #[test]
    fn test_write_layer() {
        let mut array = mock_array();

        array.timer.tries = 6;
        array.reg_pin.set_high().unwrap();
        array.write_layer(&[0x57, 0x3f], None).unwrap_or(());
        assert_eq!(array.spi.written, [0xa8, 0xc0]);
        assert_eq!(array.timer.tries, 0);
        assert_eq!(array.reg_pin.cycles, 1);
        assert_eq!(array.output_disable.cycles, 0);
        assert_eq!(array.output_disable.state, false);

        array.write_layer(&[13], Some(3)).unwrap_or(());
        assert_eq!(array.reg_pin.cycles, 2);
        assert_eq!(array.output_disable.cycles, 1);
        assert_eq!(array.output_disable.state, false);
        assert_eq!(array.row_pins.2.state, false);
        assert_eq!(array.row_pins.1.state, true);
        assert_eq!(array.row_pins.0.state, true);
    }
}
