#![feature(ascii_char)]
#![feature(adt_const_params)]
#![no_std]
#![no_main]

use arduino_hal::{DefaultClock, Delay, I2c, Usart};
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

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    uwriteln!(&mut serial, "--Init--").unwrap();

    let mut i2c_bus = I2c::new(dp.TWI, pins.a4.into_pull_up_input(), pins.a5.into_pull_up_input(), 40000);
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

    lcd.clean_display();
    lcd.return_home();
    lcd.set_cursor_blink_state(State::Off);
    lcd.set_cursor_state(State::Off);

    let mut buffer = [0u8; 20];
    let mut i: i32 = 0;

    let mut is_micro_on = true;
    let mut is_live = false;
    let mut viewers: i16 = 3000;
    let mut followers: i16 = 10;

    write_custom_chars_to_cgram(&mut lcd); // Load custom chars

    //Microphone status
    lcd.write_graph_to_pos(0, (0, 0));
    lcd.write_graph_to_pos(6, (1, 0));
    // lcd.write_char_to_pos('=', (1, 0));
    lcd.write_str_to_pos(if is_micro_on { "ON" } else { "OFF" }, (2, 0));

    //Microphone status
    lcd.write_graph_to_pos(if is_live { 2 } else { 7 }, (0, 1));
    //Live status
    lcd.write_str_to_pos(if is_live { "LIVE" } else { "offl" }, (1, 1));

    //Separators
    lcd.write_graph_to_pos(5, (5, 0));
    lcd.write_graph_to_pos(5, (5, 1));
    lcd.write_graph_to_pos(5, (11, 0));
    lcd.write_graph_to_pos(5, (11, 1));

    //Vievers
    lcd.write_graph_to_pos(3, (6, 0));
    lcd.write_char_to_pos('=', (7, 0));
    lcd.write_str_to_pos(if viewers < 999 { viewers.numtoa_str(10, &mut buffer) } else { "999" }, (8, 0));

    //Vievers
    lcd.write_graph_to_pos(4, (6, 1));
    lcd.write_char_to_pos('=', (7, 1));
    lcd.write_str_to_pos(if followers < 999 { followers.numtoa_str(10, &mut buffer) } else { "999" }, (8, 1));

    // lcd.write_graph_to_pos(1, (1, 1));
    // lcd.write_graph_to_pos(2, (2, 1));
    // lcd.write_graph_to_pos(3, (3, 1));
    // lcd.write_graph_to_pos(4, (4, 1));
    // lcd.write_graph_to_pos(5, (5, 1));
    // lcd.write_graph_to_pos(6, (6, 1));
    // lcd.write_graph_to_pos(7, (7, 1));

    // lcd.write_byte_to_pos(0, (0, 1));
    // lcd.write_byte_to_pos(1, (1, 1));
    // lcd.write_byte_to_pos(2, (2, 1));
    // lcd.write_byte_to_pos(3, (3, 1));
    // lcd.write_byte_to_pos(4, (4, 1));
    // lcd.write_byte_to_pos(5, (5, 1));
    // lcd.write_byte_to_pos(6, (6, 1));
    // lcd.write_byte_to_pos(7, (7, 1));

    // lcd.write_graph_to_cgram(0, &graph);
    // lcd.write_graph_to_pos(0, (0, 0));
    //
    // lcd.write_graph_to_cgram(4, &graph_i);
    // lcd.write_graph_to_pos(4, (1, 0));

    ///TEST////
    //     lcd.write_graph_to_cgram(0, &HEART);

    //     // to test cgram read
    //     // read heart graph from CGRAM, modify it to a diamond shape, then write it to another CGRAM address
    //     let mut graph_data = lcd.read_graph_from_cgram(0);
    //     graph_data.upper[1].set_bit(2);
    //     graph_data.upper[2].set_bit(2);
    //     graph_data.lower.as_mut().unwrap()[1].clear_bit(2);
    //     lcd.write_graph_to_cgram(1, &graph_data);
    //
    //     lcd.set_cursor_blink_state(State::On);
    //
    //     // to test function works
    //     // we set cursor 1 step right
    //     lcd.set_cursor_pos((1, 0));
    //
    //     // type writer effect
    //     lcd.typewriter_write("hello,", 250_000);
    //
    //     // relative cursor move
    //     lcd.offset_cursor_pos((1, 0));
    //
    //     // to test write string to cur pos
    //     lcd.write_str_to_cur("world!");
    //
    //     // manually delay
    //     lcd.delay_ms(250);
    //
    //     let line_capacity = lcd.get_line_capacity();
    //
    //     // to test write character to specified position
    //     // since tilde character (~) is not in CGROM of LCD1602A
    //     // it should be displayed as a full rectangle
    //     lcd.write_char_to_pos('~', (15, 0));
    //
    //     // manually delay
    //     lcd.delay_ms(250);
    //
    //     // to test whether line break works well
    //     // set cursor to the end of first line, and write a vertical line
    //     lcd.set_cursor_pos((line_capacity - 1, 0));
    //     lcd.write_char_to_cur('|');
    //
    //     // turn off cursor blinking, so that cursor will only be a underline
    //     lcd.set_cursor_blink_state(State::Off);
    //
    //     lcd.typewriter_write("Hello, ", 250_000);
    //
    //     // to test right to left write in
    //     // move cursor to left end of display window, then type string in reverse order
    //     lcd.set_direction(MoveDirection::RightToLeft);
    //     lcd.set_cursor_pos((15, 1));
    //     lcd.typewriter_write("~!", 250_000);
    //     // and the 2 type of split flap display effect
    //     lcd.split_flap_write("2061", FlipStyle::Simultaneous, None, 150_000, None);
    //     lcd.split_flap_write("DCL", FlipStyle::Sequential, Some(10), 150_000, Some(250_000));
    //
    //     lcd.set_cursor_state(State::Off);
    //
    //     // replace 2 rectangle with custom heart shape and diamond shape
    //     lcd.delay_ms(1_000);
    //     lcd.write_graph_to_pos(0, (15, 0));
    //     lcd.delay_ms(1_000);
    //     // although we define diamond shape as index 1 above,
    //     // but since we define shape in 5x11 Font, and read as 5x8 Font, the actual index should be 1*2 = 2.
    //     lcd.write_graph_to_pos(2, (15, 1));
    //
    //     // to test read from DDRAM
    //     // read from first line end, and write same character to the second line end
    //     let char_at_end = lcd.read_byte_from_pos((39, 0));
    //     lcd.write_byte_to_pos(char_at_end, (39, 1));
    //
    //     // shift display window
    //     lcd.delay_ms(1_000);
    //     lcd.shift_display_to_pos(2, MoveStyle::Shortest, State::On, 250_000);
    //     lcd.delay_ms(1_000);
    //     lcd.shift_display_to_pos(40 - 2, MoveStyle::Shortest, State::On, 250_000);
    //     lcd.delay_ms(1_000);
    //     lcd.shift_display_to_pos(0, MoveStyle::Shortest, State::On, 250_000);
    //
    //     // and blinking display 3 times
    //     lcd.delay_ms(1_000);
    //     lcd.full_display_blink(3, 500_000);
    //
    //     // and blinking backlight 3 times
    //     for _ in 0..3 {
    //         lcd.delay_ms(500);
    //         lcd.set_backlight(State::Off);
    //         lcd.delay_ms(500);
    //         lcd.set_backlight(State::On);
    //     }
    //
    //     // Set the font to Font5x11, and clean screen, and write a few words.
    //     lcd.delay_ms(1_000);
    //     lcd.set_line_mode(lcd1602_driver::command::LineMode::OneLine);
    //     lcd.set_font(lcd1602_driver::command::Font::Font5x11);
    //     lcd.clean_display();
    //     lcd.return_home();
    //     lcd.set_cursor_blink_state(State::On);
    //
    //     lcd.write_graph_to_cur(0);
    //     lcd.typewriter_write("Hello, BigFont", 200_000);
    //     lcd.write_graph_to_cur(1);
    ///TEST////
    // draw_cells(&LIVE, (0, 0), &mut lcd).unwrap();
    loop {
        // lcd.write_str_to_pos(i.numtoa_str(10, &mut buffer), (0, 1));
        if (i < 999) {
            i = i + 1;
        }

        // lcd.write_graph_to_pos(0, (0, 0));

        uwriteln!(&mut serial, "{}", i).unwrap();

        // let _ = (0..CHARS_ARR_LENGTH).zip(CUSTOM_CHARS).map(|(i, char)| {
        //     let graph = CGRAMGraph { upper: char, lower: None };
        //     uwriteln!(&mut serial, "--III--: {}", i).unwrap();
        //
        // });

        led.toggle();

        // lcd.write_str_to_pos(st.as_str(), (2, 0));
        // i = i + 1;

        arduino_hal::delay_ms(1000);
    }
}
