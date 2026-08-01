#![feature(ascii_char)]
#![feature(adt_const_params)]
#![no_std]
#![no_main]

// $env:RAVEDUDE_PORT="COM3"

use arduino_hal::clock::Clock;
use arduino_hal::hal::usart::{Event, Usart0};
use arduino_hal::port::mode::{Input, Output, PullUp};
use arduino_hal::port::Pin;
use arduino_hal::prelude::{_embedded_hal_serial_Read, _embedded_hal_serial_Write};
use arduino_hal::{DefaultClock, Delay, I2c, Usart};
use avr_device::atmega328p::Interrupt;
use avr_device::interrupt;
use core::ascii::Char;
use core::hash::Hash;
use core::ops::Index;
use core::write;
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::SevenBitAddress;
use heapless::spsc::Queue;
use heapless::{format, String, Vec};
use i2c_character_display::{
    AdafruitLCDBackpack, CharacterDisplayAIP31068, CharacterDisplayError, CharacterDisplayPCF8574T, CharacterDisplayST7032i, LcdDisplayType,
};
use ufmt::{uwrite, uwriteln};

use lcd1602_driver::command::RAMType::CGRam;
use lcd1602_driver::command::{Font, RAMType};
use lcd1602_driver::sender::SendCommand;
use lcd1602_driver::{
    command::{DataWidth, MoveDirection, State},
    lcd::{self, Anim, Basic, CGRAMGraph, Ext, ExtRead, FlipStyle, Lcd, MoveStyle},
    sender::I2cSender,
    utils::BitOps,
};
use nb::block;
use numtoa::NumToA;
type CustomCGRAM = [u8; 8];
type CustomCellChar = (CustomCGRAM, u8);
// static LED_STATE: Mutex<Cell<bool>> = Mutex::new(Cell::new(true));
const MICROPHONE_0: CustomCGRAM = [0b00100, 0b01010, 0b01010, 0b11011, 0b01010, 0b01010, 0b00100, 0b01110];
const MICROPHONE_OFF_1: CustomCGRAM = [0b00100, 0b01011, 0b01010, 0b11111, 0b01010, 0b11010, 0b00100, 0b01110];
const CIRCLE_2: CustomCGRAM = [0b00000, 0b00100, 0b01110, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const PLAY_3: CustomCGRAM = [0b00000, 0b01000, 0b01100, 0b01110, 0b01100, 0b01000, 0b00000, 0b00000];
const HEART_4: CustomCGRAM = [0b00000, 0b01010, 0b11111, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const SEPARATOR_5: CustomCGRAM = [0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110];
const DASH_6: CustomCGRAM = [0b00000, 0b00000, 0b00000, 0b01110, 0b01110, 0b00000, 0b00000, 0b00000];
const EMPTY_CIRCLE_7: CustomCGRAM = [0b00000, 0b00100, 0b01010, 0b10001, 0b01010, 0b00100, 0b00000, 0b00000];
const CUSTOM_CHARS: [CustomCGRAM; 8] = [MICROPHONE_0, MICROPHONE_OFF_1, CIRCLE_2, PLAY_3, HEART_4, SEPARATOR_5, DASH_6, EMPTY_CIRCLE_7];

const OPEN_TRANSITION_BYTE: u8 = 42;
const CLOSE_TRANSITION_BYTE: u8 = 36;

static mut RX_QUEUE: Queue<u8, 256> = Queue::new(); // Ring buffer

type LocalDisplay<'a, 'b> = Lcd<'a, 'b, I2cSender<'a, I2c, SevenBitAddress>, Delay, true>;
fn write_custom_chars_to_cgram(lcd: &mut LocalDisplay) {
    for i in 0..CUSTOM_CHARS.len() {
        let char = CUSTOM_CHARS[i];
        let graph = CGRAMGraph { upper: char, lower: None };
        lcd.write_graph_to_cgram(i.try_into().unwrap(), &graph);
    }
}

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
    // let dp = avr_device::atmega328p::Peripherals::take().unwrap();

    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 115200);

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
    fn set_viewers(&mut self, new_val: i16) {
        self.viewers = new_val;
    }
    fn set_followers(&mut self, new_val: i16) {
        self.followers = new_val;

        // *self = DeckState {
        //     viewers: self.viewers,
        //     is_micro_on: self.is_micro_on,
        //     is_live: self.is_live,
        //     followers: new_val,
        // };
    }
}
fn update_deck_display(state: &DeckState, lcd: &mut LocalDisplay, mut buffer: [u8; 20]) {
    //Microphone status
    lcd.write_graph_to_pos(if state.is_micro_on { 0 } else { 1 }, (0, 0));
    lcd.write_graph_to_pos(6, (1, 0));
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

    //Viewers
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

    //Followers
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

fn read_serial(data: u8, state: &mut DeckState, serial: &mut Usart0<DefaultClock>) {
    let data_char: Char = data.as_ascii().unwrap();

    uwriteln!(serial, "u8: {}", data).unwrap();
    uwriteln!(serial, "Char: {}", data_char.as_str()).unwrap();
    uwriteln!(serial, "").unwrap();

    // state.update(DeckState {
    //     is_live: true,
    //     is_micro_on: !state.is_micro_on,
    //     followers: state.followers,
    //     viewers: state.viewers,
    // });
}

fn send_button_signal() {}

// #[interrupt()]
// fn UART4() {
//     // Check RX register and drain into queue
//     while let Ok(byte) = uart.read() {
//         // Push byte to queue; drop on overflow
//         let _ = unsafe { RX_QUEUE.enqueue(byte) };
//     }
// }

#[arduino_hal::entry]
fn main() -> ! {
    let mut state = DeckState::default();
    let number_to_string_buffer = [0u8; 20]; //Number -> String buffer
    let mut i: i16 = 0;
    let mut j: i16 = 0;
    let dp = arduino_hal::Peripherals::take().unwrap();

    let pins = arduino_hal::pins!(dp);
    let mut serial: Usart0<DefaultClock> = arduino_hal::default_serial!(dp, pins, 115200);
    let mut led = pins.d13.into_output(); //Arduino led
                                          // let (reader, writer) = &serial.split();
    let mut delay = arduino_hal::Delay::new();

    // serial.write("123123");

    uwriteln!(&mut serial, "--Init--").unwrap();

    let mut i2c_bus = (I2c::new(dp.TWI, pins.a4.into_pull_up_input(), pins.a5.into_pull_up_input(), 750000));
    // let i2c_bus = embedded_hal_bus::i2c::CriticalSectionDevice::new(i2c);
    let mut sender = I2cSender::new(&mut i2c_bus, 0x27);
    let lcd_config = lcd::Config::default()
        .set_data_width(DataWidth::Bit4)
        .set_ram_type(RAMType::DDRam)
        .set_font(Font::Font5x8);

    // init LCD1602
    let mut lcd = Lcd::new(&mut sender, &mut delay, lcd_config, None);
    lcd.set_poll_interval(750000);
    lcd.clean_display();
    lcd.return_home();
    lcd.set_cursor_blink_state(State::Off);
    lcd.set_cursor_state(State::Off);
    lcd.set_display_state(State::Off);
    lcd.set_display_state(State::On);

    let cursor_state = lcd.get_cursor_pos();

    write_custom_chars_to_cgram(&mut lcd); // Load custom chars
    update_deck_display(&state, &mut lcd, number_to_string_buffer);

    let mut button_led: Pin<Output, arduino_hal::port::D2> = pins.d2.into_output(); // Microphone button LED
    let button_3 = pins.d3.into_pull_up_input(); // Microphone button

    let button_4 = pins.d4.into_pull_up_input();
    let button_5 = pins.d5.into_pull_up_input();
    let button_6 = pins.d6.into_pull_up_input();
    let button_7 = pins.d7.into_pull_up_input();
    let button_8 = pins.d8.into_pull_up_input();
    let button_9 = pins.d9.into_pull_up_input();

    led.toggle();

    let mut read_buffer: Vec<u8, 128> = Vec::<_, 128>::new();

    let mut is_transition_started = false;
    let mut is_transition_finished = false;

    let mut index = 0;

    let (mut reader, mut writer) = serial.split();

    loop {
        read_buffer.clear();
        // loop {
        //     // We assume that the receiving cannot fail
        //     let byte = serial.read().unwrap();
        //
        //     if read_buffer.push(byte).is_err() {
        //         uwriteln!(serial, "error: buffer full\r\n").unwrap();
        //         break;
        //     }
        //
        //     if byte == b'\r' {
        //         for byte in read_buffer.iter().rev().chain(b"\r\n") {
        //             serial.write_byte(*byte);
        //         }
        //         break;
        //     }
        // }
        // read_serial(res, &mut state, &mut serial);

        // if res != 0 {

        // if byte != 0 {
        // state.set_viewers(state.viewers + 1);
        // update_deck_display(&state, &mut lcd, buffer);

        // uwriteln!(serial, ":::::::TRANSITION STARTED::::: {}\r\n", byte_char.as_str()).unwrap();
        // uwriteln!(serial, "count {}\r\n", state.viewers).unwrap();
        // Writes while reading seem to break the read sequence

        // if byte_char == '\n'.as_ascii().unwrap() {
        // if byte == CLOSE_TRANSITION_BYTE {
        //     // uwriteln!(serial, "-------end line----- {}\r\n", byte_char.as_str()).unwrap();
        //     is_transition_finished = true;
        // }

        // if read_buffer.push(byte).is_err() {
        //     uwriteln!(serial, "error: buffer full\r\n").unwrap();
        // }
        // break;
        // break;
        // }
        // serial.flush();
        // let byte = serial.read().unwrap_or_default();
        // if (byte == OPEN_TRANSITION_BYTE) {
        //     is_transition_started = true;
        //     state.set_live(true);
        //     // uwriteln!(serial, ":::::::TRANSITION STARTED::::: {}\r\n", byte).unwrap(); // Writes while reading seem to break the read sequence
        //     update_deck_display(&state, &mut lcd, number_to_string_buffer);
        // }

        loop {
            // let byte = serial.read().unwrap_or_default();
            let byte = reader.read().unwrap_or_default();

            // serial.flush();
            // let byte_char = byte.as_ascii().unwrap();

            if (byte == OPEN_TRANSITION_BYTE) {
                is_transition_started = true;
                state.set_live(true);
                // uwriteln!(writer, ":::::::TRANSITION STARTED::::: {}\r\n", byte).unwrap(); // Writes while reading seem to break the read sequence
                update_deck_display(&state, &mut lcd, number_to_string_buffer);
            }

            if (byte != 0 && is_transition_started) {
                // if n_byte == CLOSE_TRANSITION_BYTE {
                //     uwriteln!(serial, "transfer complete\r\n").unwrap();
                //     is_transition_started = false;
                //     is_transition_finished = true;
                //
                //     // break;
                // }

                // uwriteln!(writer, "Res::::: {}\r\n", byte).unwrap();
                state.set_viewers(state.viewers + 1);

                if read_buffer.push(byte).is_err() {
                    // uwriteln!(serial, "error: buffer full\r\n").unwrap();
                    state.set_live(false);

                    // for byte in &read_buffer {
                    //     // read_buffer.push(byte).unwrap();
                    //     uwriteln!(&mut serial, "+++++++++++++++++++++++++++++++READ BUFFER++++++++++++++++++++++++++++++++").unwrap();
                    //     uwriteln!(&mut serial, "READ BUFFER: {}", byte.as_ascii().unwrap().as_str()).unwrap();
                    //     uwriteln!(&mut serial, "+++++++++++++++++++++++++++++++END++++++++++++++++++++++++++++++++").unwrap();
                    // }

                    break;
                };
                if (byte == CLOSE_TRANSITION_BYTE) {
                    is_transition_started = false;
                    is_transition_finished = true;
                    state.set_live(false);

                    break;
                }
            } else {
                state.set_live(false);

                break;
            }
        }
        //
        if is_transition_finished {
            uwriteln!(&mut writer, " BUFFER LEB: {}", read_buffer.len()).unwrap();

            for byte in &read_buffer {
                // read_buffer.push(byte).unwrap();
                uwriteln!(&mut writer, "+++++++++++++++++++++++++++++++READ BUFFER++++++++++++++++++++++++++++++++").unwrap();
                uwriteln!(&mut writer, "READ BUFFER: {}", byte.as_ascii().unwrap().as_str()).unwrap();
                uwriteln!(&mut writer, "+++++++++++++++++++++++++++++++END++++++++++++++++++++++++++++++++").unwrap();
            }
            read_buffer.clear();

            is_transition_finished = false;
        }
        // }

        // loop {
        //     // uwriteln!(serial, "Start receiving data ^^^^^^^^^^^\r\n").unwrap();
        //     let byte = serial.read().unwrap_or_default();
        //     let res_2 = serial.read().unwrap_or_default();
        //     let res_3 = serial.read().unwrap_or_default();
        //
        //     if res == 0 {
        //         if (is_init_log == true) {
        //             uwriteln!(serial, "Res 0!  {}\r\n", res).unwrap();
        //             is_init_log = false;
        //         }
        //         break;
        //     }
        //
        //     uwriteln!(serial, "Res::::: {}\r\n", res).unwrap();
        //     uwriteln!(serial, "Res:::::_2 {}\r\n", res_2).unwrap();
        //     uwriteln!(serial, "Res:::::_3 {}\r\n", res_3).unwrap();
        //     // let bytes = res.to_ne_bytes();
        //
        //     // for byte in bytes {
        //     if read_buffer.push(res).is_err() {
        //         uwriteln!(serial, "error: buffer full\r\n").unwrap();
        //         read_buffer.clear();
        //         break;
        //     };
        //
        //     // }
        //
        //     if res == 36 {
        //         uwriteln!(serial, "^^^^^^^^^^^ all data transfered\r\n").unwrap();
        //         break;
        //     }
        // }
        // serial.listen(Event::DataRegisterEmpty);
        // if (read_buffer[0] == 0x6d) {
        //     uwriteln!(&mut serial, "BUFFER[0]: {}", read_buffer[0]).unwrap();
        //
        //     if read_buffer.len() > 1 {
        //         uwriteln!(&mut serial, "BUFFER[1]: {}", read_buffer[1]).unwrap();
        //         uwriteln!(&mut serial, "BUFFERLEN {}", read_buffer.len()).unwrap();
        //
        //         if read_buffer[1] == 0x31 {
        //             state.set_micro(true);
        //             uwriteln!(&mut serial, "TRIUE").unwrap();
        //         } else {
        //             state.set_micro(false);
        //         }
        //     }
        // }

        // let concated = read_buffer.iter().map(|c|{c.as_ascii().unwrap().as_str()}).collect();

        if button_3.is_low() {
            if i < 999 {
                i = i + 2;
            }

            if j < 999 {
                j = j + 1;
            }

            state.update(DeckState {
                is_live: true,
                is_micro_on: true,
                followers: state.followers + 1,
                viewers: state.viewers + 1,
            });
        } else {
            state.update(DeckState {
                is_live: state.is_live,
                is_micro_on: false,
                followers: state.followers,
                viewers: state.viewers,
            });
        }

        if button_4.is_low() {
            // uwriteln!(&mut serial, "BUTTON 4 - on").unwrap();
            // uwriteln!(&mut serial, "word: {} || bytes: {}", res, res_bytes[0]).unwrap();
            writer.write(0x35);
        }
        if button_5.is_low() {
            // uwriteln!(&mut serial, "BUTTON 5 - on").unwrap();
            writer.write(0x35);
            uwriteln!(&mut writer, "buffer len: {}", read_buffer.len()).unwrap();
        }
        if button_6.is_low() {
            // uwriteln!(&mut serial, "BUTTON 6 - on").unwrap();
            writer.write(0x36);
        }
        if button_7.is_low() {
            // uwriteln!(&mut serial, "BUTTON 7 - on").unwrap();
            writer.write(0x37);
        }
        if button_8.is_low() {
            // uwriteln!(&mut serial, "BUTTON 8 - on").unwrap();
            writer.write(0x38);
        }
        if button_9.is_low() {
            // uwriteln!(&mut serial, "BUTTON 9 - on").unwrap();
            writer.write(0x39);
        }

        update_deck_display(&state, &mut lcd, number_to_string_buffer);
        set_button_led(state.is_micro_on, &mut button_led);

        // if read_buffer.len() <= 13 {
        //     read_buffer.clear();
        // }
    }
}
