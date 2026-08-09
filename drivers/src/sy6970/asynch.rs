use super::{ChargeStatus, SY6970, SY6970_ADDRESS, Sy6970Error, SystemStatus};
use embedded_hal_async::i2c::I2c;

const REG_INPUT_SOURCE_CONTROL: u8 = 0x00;
const REG_ADC_CONTROL: u8 = 0x02;
const REG_POWER_ON_CONFIG: u8 = 0x03;
const REG_CHARGE_TIMER_CONTROL: u8 = 0x07;
const REG_SYSTEM_STATUS: u8 = 0x0B;
const REG_BATTERY_VOLTAGE: u8 = 0x0E;
const REG_BUS_VOLTAGE: u8 = 0x11;

const EN_HIZ: u8 = 1 << 7;
// Jade sets both conversion bits at initialization. `CONV_START` is read-only
// in continuous mode, but preserving this write exactly keeps the board PMU
// configuration aligned with Jade.
const CONV_START: u8 = 1 << 7;
const CONV_RATE: u8 = 1 << 6;
const CHARGE_ENABLE: u8 = 1 << 4;
const WATCHDOG_TIMER_MASK: u8 = 0b0011_0000;
const CHARGE_STATUS_MASK: u8 = 0b0001_1000;
const CHARGE_STATUS_SHIFT: u8 = 3;
const BUS_GOOD: u8 = 1 << 7;
const BATTERY_VOLTAGE_MASK: u8 = 0x7F;
const BATTERY_VOLTAGE_BASE_MV: u16 = 2304;
const BATTERY_VOLTAGE_STEP_MV: u16 = 20;

fn battery_voltage_from_register(value: u8) -> u16 {
    let code = value & BATTERY_VOLTAGE_MASK;

    // LilyGO's driver treats an all-zero ADC code as an absent/unusable
    // battery reading. This matters during the USB-only boot decision.
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

fn system_status_from_register(system: u8, bus_voltage: u8) -> SystemStatus {
    SystemStatus {
        // `REG0B.VBUS_STAT` is an adapter classification result.  It can stay
        // latched after the cable is removed on this board.  `REG11.BUS_GD`
        // is the charger’s dedicated physical-input signal: 0 means no BUS
        // attached and 1 means a BUS is attached.  Use it for live USB state.
        usb_present: (bus_voltage & BUS_GOOD) != 0,
        charge_status: charge_status_from_register(system),
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

    /// Initializes the PMU without changing its factory charge limits.
    ///
    /// The board must not remain in HIZ mode. Continuous ADC conversion is
    /// enabled for the one-time battery-presence check, and the PMU watchdog
    /// is disabled so it cannot revert USB-only protection later.
    pub async fn init(&mut self) -> Result<(), Sy6970Error> {
        self.update_bits(REG_INPUT_SOURCE_CONTROL, EN_HIZ, 0)
            .await?;
        self.update_bits(REG_ADC_CONTROL, 0, CONV_START | CONV_RATE)
            .await?;
        self.update_bits(REG_CHARGE_TIMER_CONTROL, WATCHDOG_TIMER_MASK, 0)
            .await
    }

    /// Reads the battery-voltage ADC in millivolts.
    ///
    /// Register `0x0E` stores a seven-bit ADC code where the voltage is
    /// `2304 + 20 * code` mV.
    pub async fn get_battery_voltage_mv(&mut self) -> Result<u16, Sy6970Error> {
        let value = self.read_register(REG_BATTERY_VOLTAGE).await?;
        Ok(battery_voltage_from_register(value))
    }

    /// Reads USB-input and charge facts without changing PMU configuration.
    ///
    /// `REG11.BUS_GD` is deliberately sampled alongside `REG0B`: it reports
    /// whether a physical input is attached, while `REG0B.VBUS_STAT` reports
    /// the classified adapter type and can be stale after hot-unplug.
    pub async fn get_system_status(&mut self) -> Result<SystemStatus, Sy6970Error> {
        let system = self.read_register(REG_SYSTEM_STATUS).await?;
        let bus_voltage = self.read_register(REG_BUS_VOLTAGE).await?;
        Ok(system_status_from_register(system, bus_voltage))
    }

    /// Sets the charge-enable bit (bit 4) in `REG03`.
    pub async fn set_charge_enabled(&mut self, enabled: bool) -> Result<(), Sy6970Error> {
        if enabled {
            self.update_bits(REG_POWER_ON_CONFIG, 0, CHARGE_ENABLE)
                .await
        } else {
            self.update_bits(REG_POWER_ON_CONFIG, CHARGE_ENABLE, 0)
                .await
        }
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
    fn decodes_live_bus_good_status() {
        // Adapter classification alone is not physical cable detection.
        assert!(!system_status_from_register(0xA4, 0x00).usb_present);
        assert!(system_status_from_register(0x00, BUS_GOOD).usb_present);
        assert!(system_status_from_register(0xE4, BUS_GOOD).usb_present);
    }
}
