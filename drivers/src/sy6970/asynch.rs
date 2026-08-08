use super::{ChargeStatus, SY6970, SY6970_ADDRESS, Sy6970Error, SystemStatus};
use embedded_hal_async::i2c::I2c;

const REG_INPUT_SOURCE_CONTROL: u8 = 0x00;
const REG_ADC_CONTROL: u8 = 0x02;
const REG_POWER_ON_CONFIG: u8 = 0x03;
const REG_CHARGE_TIMER_CONTROL: u8 = 0x07;
const REG_SYSTEM_STATUS: u8 = 0x0B;
const REG_BATTERY_VOLTAGE: u8 = 0x0E;

const EN_HIZ: u8 = 1 << 7;
const ADC_ENABLE: u8 = 1 << 7;
const ADC_CONTINUOUS: u8 = 1 << 6;
const CHARGE_ENABLE: u8 = 1 << 4;
const WATCHDOG_TIMER_MASK: u8 = 0b0011_0000;
const CHARGE_STATUS_MASK: u8 = 0b0001_1000;
const CHARGE_STATUS_SHIFT: u8 = 3;
const VBUS_STATUS_MASK: u8 = 0b1110_0000;
const VBUS_STATUS_SHIFT: u8 = 5;
const VBUS_STATUS_NO_INPUT: u8 = 0;
const VBUS_STATUS_OTG: u8 = 7;
const INPUT_POWER_GOOD: u8 = 1 << 2;
const BATTERY_VOLTAGE_MASK: u8 = 0x7F;
const BATTERY_VOLTAGE_BASE_MV: u16 = 2304;
const BATTERY_VOLTAGE_STEP_MV: u16 = 20;

fn battery_voltage_from_register(value: u8) -> u16 {
    let code = value & BATTERY_VOLTAGE_MASK;

    // LilyGO's reference SY6970 driver treats an all-zero ADC code as no
    // usable battery reading rather than the nominal ADC base voltage. Mirror
    // that board-level convention so firmware can apply LilyGO's USB-only
    // power-path workaround.
    if code == 0 {
        0
    } else {
        BATTERY_VOLTAGE_BASE_MV + u16::from(code) * BATTERY_VOLTAGE_STEP_MV
    }
}

fn charge_status_from_register(value: u8) -> ChargeStatus {
    match (value & CHARGE_STATUS_MASK) >> CHARGE_STATUS_SHIFT {
        0 => ChargeStatus::NotCharging,
        1 => ChargeStatus::PreCharge,
        2 => ChargeStatus::FastCharge,
        3 => ChargeStatus::Done,
        _ => unreachable!("charge status is a two-bit field"),
    }
}

fn system_status_from_register(value: u8) -> SystemStatus {
    let vbus_status = (value & VBUS_STATUS_MASK) >> VBUS_STATUS_SHIFT;

    SystemStatus {
        // A nonzero VBUS state means an external input except for the OTG
        // state, where the chip is sourcing VBUS rather than receiving USB
        // power. The T-Display-S3 Pro does not normally use OTG, but treating
        // it correctly keeps the UI's USB indicator truthful.
        usb_present: vbus_status != VBUS_STATUS_NO_INPUT && vbus_status != VBUS_STATUS_OTG,
        input_power_good: (value & INPUT_POWER_GOOD) != 0,
        charge_status: charge_status_from_register(value),
    }
}

impl<I2C> SY6970<I2C>
where
    I2C: I2c,
{
    async fn read_register(&mut self, register: u8) -> Result<u8, Sy6970Error> {
        let mut value = [0u8];
        self.i2c
            .write_read(SY6970_ADDRESS, &[register], &mut value)
            .await?;
        Ok(value[0])
    }

    async fn write_register(&mut self, register: u8, value: u8) -> Result<(), Sy6970Error> {
        self.i2c.write(SY6970_ADDRESS, &[register, value]).await?;
        Ok(())
    }

    /// Updates selected bits without changing the chip's other configuration.
    async fn update_bits(
        &mut self,
        register: u8,
        clear_mask: u8,
        set_mask: u8,
    ) -> Result<(), Sy6970Error> {
        let value = self.read_register(register).await?;
        self.write_register(register, (value & !clear_mask) | set_mask)
            .await
    }

    /// Initializes the charger without resetting it or overwriting its factory
    /// charging limits.
    ///
    /// The configuration exits HIZ mode (register `0x00`), enables continuous
    /// battery-voltage conversion (register `0x02`), and disables the watchdog
    /// timer (register `0x07`).
    pub async fn init(&mut self) -> Result<(), Sy6970Error> {
        self.update_bits(REG_INPUT_SOURCE_CONTROL, EN_HIZ, 0)
            .await?;
        self.update_bits(REG_ADC_CONTROL, 0, ADC_ENABLE | ADC_CONTINUOUS)
            .await?;
        self.update_bits(REG_CHARGE_TIMER_CONTROL, WATCHDOG_TIMER_MASK, 0)
            .await
    }

    /// Reads the battery voltage ADC in millivolts.
    ///
    /// Register `0x0E` stores a seven-bit ADC code where the voltage is
    /// `2304 + 20 * code` mV.
    pub async fn get_battery_voltage(&mut self) -> Result<u16, Sy6970Error> {
        let value = self.read_register(REG_BATTERY_VOLTAGE).await?;
        Ok(battery_voltage_from_register(value))
    }

    /// Alias for [`Self::get_battery_voltage`] that makes the unit explicit.
    pub async fn get_battery_voltage_mv(&mut self) -> Result<u16, Sy6970Error> {
        self.get_battery_voltage().await
    }

    /// Reads the current battery charge phase from bits 4:3 of register `0x0B`.
    pub async fn get_charge_status(&mut self) -> Result<ChargeStatus, Sy6970Error> {
        Ok(self.get_system_status().await?.charge_status)
    }

    /// Reads USB-input and charge facts from the system-status register.
    pub async fn get_system_status(&mut self) -> Result<SystemStatus, Sy6970Error> {
        let value = self.read_register(REG_SYSTEM_STATUS).await?;
        Ok(system_status_from_register(value))
    }

    /// Returns whether the charger is currently in a pre-charge or fast-charge
    /// phase.
    pub async fn is_charging(&mut self) -> Result<bool, Sy6970Error> {
        Ok(self.get_charge_status().await?.is_charging())
    }

    /// Returns whether the PMU detects USB input power (not an OTG output).
    pub async fn is_usb_power_present(&mut self) -> Result<bool, Sy6970Error> {
        Ok(self.get_system_status().await?.usb_present)
    }

    /// Sets the charge-enable bit (bit 4) in register `0x03`.
    pub async fn set_charge_enabled(&mut self, enabled: bool) -> Result<(), Sy6970Error> {
        if enabled {
            self.update_bits(REG_POWER_ON_CONFIG, 0, CHARGE_ENABLE)
                .await
        } else {
            self.update_bits(REG_POWER_ON_CONFIG, CHARGE_ENABLE, 0)
                .await
        }
    }

    /// Returns whether charging is enabled in register `0x03`.
    pub async fn is_charge_enabled(&mut self) -> Result<bool, Sy6970Error> {
        Ok((self.read_register(REG_POWER_ON_CONFIG).await? & CHARGE_ENABLE) != 0)
    }

    /// Alias for [`Self::is_charge_enabled`].
    pub async fn get_charge_enabled(&mut self) -> Result<bool, Sy6970Error> {
        self.is_charge_enabled().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_battery_voltage_adc_register() {
        assert_eq!(battery_voltage_from_register(0x00), 0);
        assert_eq!(battery_voltage_from_register(0x01), 2324);
        assert_eq!(battery_voltage_from_register(0xFF), 4844);
    }

    #[test]
    fn decodes_charge_status_bits() {
        assert_eq!(charge_status_from_register(0x00), ChargeStatus::NotCharging);
        assert_eq!(charge_status_from_register(0x08), ChargeStatus::PreCharge);
        assert_eq!(charge_status_from_register(0x10), ChargeStatus::FastCharge);
        assert_eq!(charge_status_from_register(0x18), ChargeStatus::Done);
    }

    #[test]
    fn decodes_usb_input_and_power_good_status() {
        let unplugged = system_status_from_register(0x00);
        assert!(!unplugged.usb_present);
        assert!(!unplugged.input_power_good);

        let usb_input = system_status_from_register(0x24);
        assert!(usb_input.usb_present);
        assert!(usb_input.input_power_good);

        let otg_output = system_status_from_register(0xE4);
        assert!(!otg_output.usb_present);
        assert!(otg_output.input_power_good);
    }
}
