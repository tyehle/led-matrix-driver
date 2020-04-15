
#[cfg(test)]

use embedded_hal as hal;
use heapless;
use heapless::consts::*;

pub struct MockPin {
    pub state: bool,
}

impl hal::digital::v2::OutputPin for MockPin {
    type Error = ();

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.state = false;
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.state = true;
        Ok(())
    }
}

pub struct MockTimer {
    pub tries: i32
}

impl hal::timer::CountDown for MockTimer {
    type Time = i32;

    fn start<T>(&mut self, duration: T) where T: Into<Self::Time> {
        self.tries = duration.into();
    }

    fn wait(&mut self) -> Result<(), nb::Error<void::Void>> {
        if self.tries > 0 {
            self.tries -= 1;
            Err(nb::Error::WouldBlock)
        } else {
            Ok(())
        }
    }
}

pub struct MockSPI {
    pub written: heapless::Vec<u8, U64>,
}

impl hal::spi::FullDuplex<u8> for MockSPI {
    type Error = ();

    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> {
        // crash if we try to read
        Err(nb::Error::Other(()))
    }

    fn send(&mut self, word: u8) -> Result<(), nb::Error<Self::Error>> {
        self.written.push(word).map_err(|_| nb::Error::Other(()))
    }
}

mod test {
    use super::*;
    use hal::prelude::*;
    use hal::digital::v2::OutputPin;
    use nb::block;

    #[test]
    fn test_mock_pin() {
        let mut pin = MockPin { state: false };

        pin.set_low().unwrap();
        assert_eq!(pin.state, false);

        pin.set_high().unwrap();
        assert_eq!(pin.state, true);
    }

    #[test]
    fn test_timer() {
        let mut timer = MockTimer { tries: 0 };
        timer.start(1);
        assert_eq!(timer.wait(), Err(nb::Error::WouldBlock));
        assert_eq!(timer.wait(), Ok(()));

        timer.start(100);
        assert_eq!(block!(timer.wait()), Ok(()));
    }

    #[test]
    fn test_mock_spi() {
        let mut bus = MockSPI { written: heapless::Vec::new() };

        bus.send(0u8).unwrap();
        bus.send(157u8).unwrap();
        assert_eq!(&bus.written, &[0, 157]);

        assert_eq!(bus.read(), Err(nb::Error::Other(())));
    }
}