#![no_std]
#![no_main]

use arduino_hal::i2c::Error;
use i2c_character_display::{
    AdafruitLCDBackpack, CharacterDisplayAIP31068, CharacterDisplayError, CharacterDisplayPCF8574T, CharacterDisplayST7032i, LcdDisplayType,
};
use ufmt::{uwrite, uwriteln};

#[cfg(not(doc))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // disable interrupts - firmware has panicked so no ISRs should continue running
    avr_device::interrupt::disable();
    // get the peripherals so we can access serial and the LED.
    //
    // SAFETY: Because main() already has references to the peripherals this is an unsafe
    // operation - but because no other code can run after the panic handler was called,
    // we know it is okay.
    let dp = unsafe { arduino_hal::Peripherals::steal() };
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    ufmt::uwriteln!(&mut serial, " Firmware panic!\r").unwrap();

    // Accessing the panic info unfortunately means that the optimizer can no longer remove panic
    // messages from the resulting binary.  This leads to an explosion of SRAM usage, quickly
    // surpassing available space.
    //
    // If you need precise panic info, currently your best bet is disabling `overflow-checks` and
    // `debug-assertions` in the build profile and structuring your code such that panics never
    // include a message payload.  For example, instead of calling `.unwrap()`, use these macros:
    //
    // #[macro_export]
    // macro_rules! unwrap_result {
    //     ($v:expr) => {
    //         match $v {
    //             Ok(v) => v,
    //             Err(_) => panic!(),
    //         }
    //     };
    // }
    //
    // #[macro_export]
    // macro_rules! unwrap_option {
    //     ($v:expr) => {
    //         match $v {
    //             Some(v) => v,
    //             None => panic!(),
    //         }
    //     };
    // }

    // Print panic location:
    // if let Some(loc) = info.location() {
    //     ufmt::uwriteln!(
    //         &mut serial,
    //         "  At {}:{}:{}\r",
    //         loc.file(),
    //         loc.line(),
    //         loc.column(),
    //     )
    //     .unwrap_infallible();
    // }

    // Blink LED rapidly
    let mut led = pins.d13.into_output();
    loop {
        led.toggle();
        arduino_hal::delay_ms(100);
    }
}

// impl Debug for I2c<dyn Clock> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
//         panic(f.into());
//
//         Result::Ok(())
//     }
// }

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    /*
     * For examples (and inspiration), head to
     *
     *     https://github.com/Rahix/avr-hal/tree/main/examples
     *
     * NOTE: Not all examples were ported to all boards!  There is a good chance though, that code
     * for a different board can be adapted for yours.  The Arduino Uno currently has the most
     * examples available.
     */

    let i2c_bus = arduino_hal::i2c::I2c::new(dp.TWI, pins.a4.into_pull_up_input(), pins.a5.into_pull_up_input(), 50000);
    // let i2c_bus = embedded_hal_bus::i2c::CriticalSectionDevice::new(i2c);

    let mut led = pins.d13.into_output();
    let delay = arduino_hal::Delay::new();

    let mut lcd = CharacterDisplayPCF8574T::new(i2c_bus, LcdDisplayType::Lcd16x2, delay);

    if let Err(e) = lcd.init() {
        match e {
            CharacterDisplayError::I2cError(er) => {
                match er {
                    Error::ArbitrationLost => {
                        uwrite!(&mut serial, " !!!Error loading (I2cError): ArbitrationLost \n").unwrap();
                    }
                    Error::AddressNack => {
                        uwrite!(&mut serial, " !!!Error loading (I2cError): AddressNack \n").unwrap();
                    }
                    Error::DataNack => {
                        uwrite!(&mut serial, " !!!Error loading (I2cError): DataNack \n").unwrap();
                    }
                    Error::BusError => {
                        uwrite!(&mut serial, " !!!Error loading (I2cError): BusError \n").unwrap();
                    }
                    Error::Unknown => {
                        uwrite!(&mut serial, " !!!Error loading (I2cError): Unknown \n").unwrap();
                    }
                }
                // uwrite!(&mut serial, " !!!Error loading (I2cError): {}\n", er).unwrap();
            }
            rest_error => {
                uwrite!(&mut serial, " !!!Error loading LCD: {}\n", rest_error).unwrap();
            }
        }
    }

    if let Err(e) = lcd.backlight(true) {
        uwriteln!(&mut serial, " !!!Error backlight: {}\n", e).unwrap();

        panic!(" Error backlight: {}\n", e);
    }

    if let Err(e) = lcd.clear() {
        panic!(" Error clear: {}\n", e);
    }
    // lcd.home()?;
    //
    //
    // lcd.show_cursor(true);
    // lcd.blink_cursor(true);

    if let Err(e) = lcd.print("222 - 402") {
        panic!(" Error backlight: {}\n", e);
    }

    // let mut serial = arduino_hal::default_serial!(dp, pins, 115200);
    //
    // ufmt::uwriteln!(&mut serial, "Hello from Arduino!\r").unwrap_infallible();
    // ufmt::uwriteln!(&mut serial, "Panic in 5 seconds!\r").unwrap_infallible();

    // arduino_hal::delay_ms(5000);

    // Panic messages cannot yet be captured because they rely on core::fmt
    // which is way too big for AVR

    loop {
        led.toggle();

        arduino_hal::delay_ms(1000);
    }
}
