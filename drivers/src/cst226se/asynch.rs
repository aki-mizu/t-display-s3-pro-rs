use super::{
    CST226SE, CST226SE_ADDRESS, RAW_TOUCH_REPORT_LEN, TouchPoint, TouchSensorError,
    has_valid_firmware_checkcode, parse_first_point, should_acknowledge_empty_report,
};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::I2c;

impl<I2C, PIN, RST, DELAY> CST226SE<I2C, PIN, RST, DELAY>
where
    I2C: I2c,
    PIN: InputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn write_register(&mut self, register_and_value: &[u8]) -> Result<(), TouchSensorError> {
        self.i2c.write(CST226SE_ADDRESS, register_and_value).await?;
        Ok(())
    }

    async fn read_register(
        &mut self,
        register: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), TouchSensorError> {
        self.i2c
            .write_read(CST226SE_ADDRESS, register, buffer)
            .await?;
        Ok(())
    }

    async fn read_report(&mut self) -> Result<[u8; RAW_TOUCH_REPORT_LEN], TouchSensorError> {
        let mut report = [0u8; RAW_TOUCH_REPORT_LEN];
        self.read_register(&[0x00], &mut report).await?;
        Ok(report)
    }

    async fn acknowledge_empty_report(&mut self) -> Result<(), TouchSensorError> {
        self.write_register(&[0x00, 0xAB]).await
    }

    /// Resets the controller and verifies its firmware checkcode.
    ///
    /// The command-mode sequence follows LilyGO's CST226SE reference driver.
    /// It also leaves the controller in normal report mode before returning.
    pub async fn begin(&mut self) -> Result<(), TouchSensorError> {
        self.reset().await?;

        // Enter command mode and read the four-byte firmware checkcode at
        // command address D1:FC. Always try to return to normal report mode,
        // even if the read fails.
        self.write_register(&[0xD1, 0x01]).await?;
        self.delay.delay_ms(10).await;

        let mut checkcode = [0u8; 4];
        let read_result = self.read_register(&[0xD1, 0xFC], &mut checkcode).await;
        let exit_result = self.write_register(&[0xD1, 0x09]).await;

        read_result?;
        exit_result?;

        if !has_valid_firmware_checkcode(checkcode) {
            return Err(TouchSensorError::InvalidDevice);
        }

        Ok(())
    }

    /// Resets the controller with the timing used by LilyGo's CST226SE driver.
    pub async fn reset(&mut self) -> Result<(), TouchSensorError> {
        if let Some(rst) = &mut self.rst_pin {
            rst.set_low().map_err(|_| TouchSensorError::PinError)?;
            self.delay.delay_ms(100).await;
            rst.set_high().map_err(|_| TouchSensorError::PinError)?;
            self.delay.delay_ms(100).await;
        }
        Ok(())
    }

    /// Returns `true` when the active-low interrupt pin signals touch data.
    pub fn is_touch_available(&mut self) -> Result<bool, TouchSensorError> {
        self.touch_int
            .is_low()
            .map_err(|_| TouchSensorError::PinError)
    }

    /// Reads the complete controller report and returns its first active point.
    ///
    /// A report without an active touch, the controller's home-button report,
    /// or an invalid report returns `Ok(None)`. Empty valid reports are
    /// acknowledged as required by LilyGo's reference driver.
    pub async fn read_first_point(&mut self) -> Result<Option<TouchPoint>, TouchSensorError> {
        let report = self.read_report().await?;

        if should_acknowledge_empty_report(&report) {
            self.acknowledge_empty_report().await?;
        }

        Ok(parse_first_point(&report))
    }

    /// Alias for [`Self::read_first_point`] used by the application touch path.
    pub async fn read_touch(&mut self) -> Result<Option<TouchPoint>, TouchSensorError> {
        self.read_first_point().await
    }
}
