#![feature(ascii_char)]
#![feature(adt_const_params)]
#![no_std]
#![no_main]

use arduino_hal::port::mode::{Input, Output, PullUp};
use arduino_hal::port::Pin;
use arduino_hal::{DefaultClock, Delay, I2c, Usart};
use avr_device::atmega328p::portb::PINB;
use avr_device::atmega328p::TWI;
use core::ascii::Char;
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::SevenBitAddress;
use heapless::{format, String};
use i2c_character_display::{
    AdafruitLCDBackpack, CharacterDisplayAIP31068, CharacterDisplayError, CharacterDisplayPCF8574T, CharacterDisplayST7032i, LcdDisplayType,
};
use ufmt::{uwrite, uwriteln};

use lcd1602_driver::command::RAMType::CGRam;
use lcd1602_driver::command::{Font, RAMType};
use lcd1602_driver::{
    command::{DataWidth, MoveDirection, State},
    lcd::{self, Anim, Basic, CGRAMGraph, Ext, ExtRead, FlipStyle, Lcd, MoveStyle},
    sender::I2cSender,
    utils::BitOps,
};
use numtoa::NumToA;

// const HEART: CGRAMGraph = CGRAMGraph {
//     upper: [0b00000, 0b00000, 0b01010, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000],
//     lower: Some([0b00100, 0b01110, 0b00100]),
// };

type CustomCGRAM = [u8; 8];
type CustomCellChar = (CustomCGRAM, u8);
// const MICROPHONE: CustomCellChar = ([0b00100, 0b01010, 0b01010, 0b11011, 0b01010, 0b01010, 0b00100, 0b01110], 0);
// const MICROPHONE_OFF: CustomCellChar = ([0b00100, 0b01011, 0b01010, 0b11111, 0b01010, 0b11010, 0b00100, 0b01110], 1);
// const CIRCLE: CustomCellChar = ([0b00000, 0b00100, 0b01110, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000], 2);
// const PLAY: CustomCellChar = ([0b00000, 0b00000, 0b01000, 0b01100, 0b01110, 0b01100, 0b01000, 0b00000], 3);
// const HEART: CustomCellChar = ([0b00000, 0b00000, 0b01010, 0b11111, 0b11111, 0b01110, 0b00100, 0b00000], 4);
// const SEPARATOR: CustomCellChar = ([0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110], 5);
// const DASH: CustomCellChar = ([0b00000, 0b00000, 0b00000, 0b01110, 0b01110, 0b00000, 0b00000, 0b00000], 6);

const MICROPHONE_0: CustomCGRAM = [0b00100, 0b01010, 0b01010, 0b11011, 0b01010, 0b01010, 0b00100, 0b01110];
const MICROPHONE_OFF_1: CustomCGRAM = [0b00100, 0b01011, 0b01010, 0b11111, 0b01010, 0b11010, 0b00100, 0b01110];
const CIRCLE_2: CustomCGRAM = [0b00000, 0b00100, 0b01110, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const PLAY_3: CustomCGRAM = [0b00000, 0b01000, 0b01100, 0b01110, 0b01100, 0b01000, 0b00000, 0b00000];
const HEART_4: CustomCGRAM = [0b00000, 0b01010, 0b11111, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const SEPARATOR_5: CustomCGRAM = [0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110];
const DASH_6: CustomCGRAM = [0b00000, 0b00000, 0b00000, 0b01110, 0b01110, 0b00000, 0b00000, 0b00000];
const EMPTY_CIRCLE_7: CustomCGRAM = [0b00000, 0b00100, 0b01010, 0b10001, 0b01010, 0b00100, 0b00000, 0b00000];
const CUSTOM_CHARS: [CustomCGRAM; 8] = [MICROPHONE_0, MICROPHONE_OFF_1, CIRCLE_2, PLAY_3, HEART_4, SEPARATOR_5, DASH_6, EMPTY_CIRCLE_7];

type LocalDisplay<'a, 'b> = Lcd<'a, 'b, I2cSender<'a, I2c, SevenBitAddress>, Delay, true>;
fn write_custom_chars_to_cgram(lcd: &mut LocalDisplay) {
    for i in 0..CUSTOM_CHARS.len() {
        let char = CUSTOM_CHARS[i];
        let graph = CGRAMGraph { upper: char, lower: None };
        lcd.write_graph_to_cgram(i.try_into().unwrap(), &graph);
    }
}

// const HEART: CGRAMGraph = CGRAMGraph {
//     upper: [0b00000, 0b00000, 0b01010, 0b11111, 0b11111, 0b01110, 0b00100, 0b11011],
//     lower: Some([0b00000, 0b00000, 0b00000]),
// };

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

    uwriteln!(&mut serial, " Firmware panic!\r").unwrap();

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
#[derive(Default)]
struct DeckState {
    is_micro_on: bool,
    is_live: bool,
    viewers: i16,
    followers: i16,
}

impl DeckState {
    fn update(&mut self, new_state: DeckState) {
        *self = new_state
    }

    fn toggle_micro(&mut self) {
        self.is_micro_on = !self.is_micro_on;
    }

    fn set_micro(&mut self, new_val: bool) {
        self.is_micro_on = new_val;
    }

    fn set_live(&mut self, new_val: bool) {
        self.is_micro_on = new_val;
    }
}
fn update_deck_display(state: &DeckState, lcd: &mut LocalDisplay, mut buffer: [u8; 20]) {
    //Microphone status
    lcd.write_graph_to_pos(if state.is_micro_on { 0 } else { 1 }, (0, 0));
    lcd.write_graph_to_pos(6, (1, 0));
    // lcd.write_char_to_pos('=', (1, 0));
    lcd.write_str_to_pos(if state.is_micro_on { "ON " } else { "OFF" }, (2, 0));

    //Microphone status
    lcd.write_graph_to_pos(if state.is_live { 2 } else { 7 }, (0, 1));
    //Live status
    lcd.write_str_to_pos(if state.is_live { "LIVE" } else { "----" }, (1, 1));

    //Separators
    lcd.write_graph_to_pos(5, (5, 0));
    lcd.write_graph_to_pos(5, (5, 1));
    lcd.write_graph_to_pos(5, (11, 0));
    lcd.write_graph_to_pos(5, (11, 1));

    //Vievers
    lcd.write_graph_to_pos(3, (6, 0));
    lcd.write_char_to_pos('=', (7, 0));
    lcd.write_str_to_pos(
        if state.viewers < 999 {
            state.viewers.numtoa_str(10, &mut buffer)
        } else {
            "999"
        },
        (8, 0),
    );

    //Vievers
    lcd.write_graph_to_pos(4, (6, 1));
    lcd.write_char_to_pos('=', (7, 1));
    lcd.write_str_to_pos(
        if state.followers < 999 {
            state.followers.numtoa_str(10, &mut buffer)
        } else {
            "999"
        },
        (8, 1),
    );
}

fn set_button_led(is_micro_on: bool, button_led: &mut Pin<Output, arduino_hal::port::D2>) {
    if !is_micro_on {
        button_led.set_high();
    } else {
        button_led.set_low();
    }
}

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    uwriteln!(&mut serial, "--Init--").unwrap();

    let mut i2c_bus = I2c::new(dp.TWI, pins.a4.into_pull_up_input(), pins.a5.into_pull_up_input(), 750000);
    // let i2c_bus = embedded_hal_bus::i2c::CriticalSectionDevice::new(i2c);

    let mut sender = I2cSender::new(&mut i2c_bus, 0x27);
    let lcd_config = lcd::Config::default()
        .set_data_width(DataWidth::Bit4)
        .set_ram_type(RAMType::DDRam)
        .set_font(Font::Font5x8);

    let mut delay = arduino_hal::Delay::new();
    // init LCD1602
    let mut lcd = Lcd::new(&mut sender, &mut delay, lcd_config, None);

    let mut led = pins.d13.into_output();
    let mut button_led: Pin<Output, arduino_hal::port::D2> = pins.d2.into_output();
    let button_1 = pins.d3.into_pull_up_input();

    // lcd.clean_display();
    lcd.return_home();
    lcd.set_cursor_blink_state(State::Off);
    lcd.set_cursor_state(State::Off);

    let mut state = DeckState::default();
    let buffer = [0u8; 20];
    let mut i: i16 = 0;
    let mut j: i16 = 0;

    write_custom_chars_to_cgram(&mut lcd); // Load custom chars

    update_deck_display(&state, &mut lcd, buffer);
    led.toggle();

    loop {
        // if (i < 999) {
        //     i = i + 2;
        // }
        //
        // if (j < 999) {
        //     j = j + 1;
        // }

        if button_1.is_low() {
            uwriteln!(&mut serial, "low = {}", i).unwrap();

            if (i < 999) {
                i = i + 2;
            }

            if (j < 999) {
                j = j + 1;
            }

            state.update(DeckState {
                is_live: true,
                is_micro_on: true,
                followers: i,
                viewers: j,
            });
        } else {
            uwriteln!(&mut serial, "high = {}", i).unwrap();

            state.update(DeckState {
                is_live: false,
                is_micro_on: false,
                followers: i,
                viewers: j,
            });
        }

        // state.update(DeckState {
        //     is_live: true,
        //     is_micro_on: false,
        //     followers: i,
        //     viewers: j,
        // });
        update_deck_display(&state, &mut lcd, buffer);
        // set_button_led(state.is_micro_on, &mut button_led);

        uwriteln!(&mut serial, "i = {}", i).unwrap();

        // state.update(DeckState {
        //     is_live: false,
        //     is_micro_on: true,
        //     followers: i,
        //     viewers: j,
        // });
        update_deck_display(&state, &mut lcd, buffer);
        set_button_led(state.is_micro_on, &mut button_led);
    }
}
