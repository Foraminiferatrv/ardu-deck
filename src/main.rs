#![feature(ascii_char)]
// #![feature(adt_const_params)]
// #![feature(abi_avr_interrupt)]
#![no_std]
#![no_main]

use arduino_hal::hal::usart::{Usart0};
use arduino_hal::port::mode::Output;
use arduino_hal::port::Pin;
use arduino_hal::prelude::{_embedded_hal_serial_Read};
use arduino_hal::{DefaultClock, Delay, I2c};
use core::ascii::Char;
use embedded_hal::i2c::SevenBitAddress;
use heapless::{String, Vec};

use ufmt::{uWrite, uwriteln};

use lcd1602_driver::command::{Font, RAMType};
use lcd1602_driver::{
    command::{DataWidth, State},
    lcd::{self, Basic, CGRAMGraph, Ext, Lcd},
    sender::I2cSender,
};
type CustomCGRAM = [u8; 8];

const BAUDRATE: u32 = 115200;

const MICROPHONE_0: CustomCGRAM = [0b00100, 0b01010, 0b01010, 0b11011, 0b01010, 0b01010, 0b00100, 0b01110];
const MICROPHONE_OFF_1: CustomCGRAM = [0b00100, 0b01011, 0b01010, 0b11111, 0b01010, 0b11010, 0b00100, 0b01110];
const CIRCLE_2: CustomCGRAM = [0b00000, 0b00100, 0b01110, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const PLAY_3: CustomCGRAM = [0b00000, 0b01000, 0b01100, 0b01110, 0b01100, 0b01000, 0b00000, 0b00000];
const HEART_4: CustomCGRAM = [0b00000, 0b01010, 0b11111, 0b11111, 0b01110, 0b00100, 0b00000, 0b00000];
const SEPARATOR_5: CustomCGRAM = [0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110, 0b01110];
const DASH_6: CustomCGRAM = [0b00000, 0b00000, 0b00000, 0b01110, 0b01110, 0b00000, 0b00000, 0b00000];
const EMPTY_CIRCLE_7: CustomCGRAM = [0b00000, 0b00100, 0b01010, 0b10001, 0b01010, 0b00100, 0b00000, 0b00000];
const CUSTOM_CHARS: [CustomCGRAM; 8] = [MICROPHONE_0, MICROPHONE_OFF_1, CIRCLE_2, PLAY_3, HEART_4, SEPARATOR_5, DASH_6, EMPTY_CIRCLE_7];

const OPEN_TRANSITION_BYTE: u8 = 42;//*
const MICROPHONE_BYTE: u8 =77;//M
const LIVE_BYTE: u8 =76;//L
const VIEWERS_BYTE: u8 =86;//V
const FOLLOWERS_BYTE: u8 =70;//F
const HOURS_BYTE: u8 =70;//F
const MINUTES_BYTE: u8 =70;//F
const CLOSE_TRANSITION_BYTE: u8 = 36;//$



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
    let mut serial = arduino_hal::default_serial!(dp, pins, BAUDRATE);

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
struct DeckState{
    is_micro_on: bool,
    is_live: bool,
    // viewers: i16,
    // followers: i16,
    viewers:  String<3>,
    followers: String<3>,
}

impl DeckState {
    fn default() -> DeckState{
        DeckState { is_micro_on: false, is_live: false, viewers: String::<3>::try_from("000").unwrap(), followers: String::<3>::try_from("000").unwrap() }
    }

    fn update(&mut self, new_state: DeckState) {
        *self = new_state
    }

    fn set_micro(&mut self, new_val: bool) {
        self.is_micro_on = new_val;
    }

    fn set_live(&mut self, new_val: bool) {
        self.is_live = new_val;
    }

    fn set_viewers(&mut self, new_val: &String<3>) {
        self.viewers = new_val.clone();
    }

    fn set_followers(&mut self, new_val: &String<3>) {
        self.followers = new_val.clone();
    }
}


fn update_deck_display(state: &DeckState,lcd: &mut LocalDisplay) {
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
    // lcd.write_char_to_pos('~', (11, 0));
    // lcd.write_char_to_pos('~', (11, 1));

    //Viewers
    lcd.write_graph_to_pos(3, (6, 0));
    lcd.write_char_to_pos('=', (7, 0));
    lcd.write_str_to_pos(
        if state.viewers.len() == 3 {
            // state.viewers.numtoa_str(10, &mut buffer)
            state.viewers.as_str()
        } else {
            "---"
        },
        (8, 0),
    );

    //Followers
    lcd.write_graph_to_pos(4, (6, 1));
    lcd.write_char_to_pos('=', (7, 1));
    lcd.write_str_to_pos(
        if state.viewers.len() == 3 {
            // state.followers.numtoa_str(10, &mut buffer)
            state.followers.as_str()
        } else {
            "---"
        },
        (8, 1),
    );

    //TODO:Time
    // lcd.write_str_to_pos(
    //     "0",
    //     // if state.viewers.len() == 3 {
    //         // state.followers.numtoa_str(10, &mut buffer)
    //         // state.followers.as_str("0")
    //     // } else {
    //         // "---"
    //     // },
    //     (12, 0),
    // );
    // lcd.write_char_to_pos(':', (13, 0));
    // lcd.write_char_to_pos('0', (14, 0));
    // lcd.write_char_to_pos('0', (15, 0));

    //Placeholder
    lcd.write_char_to_pos('~', (12, 1));
    lcd.write_char_to_pos('~', (13, 1));
    lcd.write_char_to_pos('~', (14, 1));
    lcd.write_char_to_pos('~', (15, 1));
    lcd.write_char_to_pos('~', (12, 0));
    lcd.write_char_to_pos('~', (13, 0));
    lcd.write_char_to_pos('~', (14, 0));
    lcd.write_char_to_pos('~', (15, 0));
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
}

fn send_button_signal() {}

#[arduino_hal::entry]
fn main() -> ! {
    let mut state = DeckState::default();
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let rx = pins.d1.into_output();
    let mut serial: Usart0<DefaultClock> = arduino_hal::Usart::new(dp.USART0, pins.d0, rx, arduino_hal::hal::usart::BaudrateExt::into_baudrate(BAUDRATE));
    let mut string_buffer = String::<3>::new();

    let mut led = pins.d13.into_output(); //Arduino led
                                          // let (reader, writer) = &serial.split();
    let mut delay = arduino_hal::Delay::new();

    let mut i2c_bus = I2c::new(dp.TWI, pins.a4.into_pull_up_input(), pins.a5.into_pull_up_input(), 750000);
    let mut sender = I2cSender::new(&mut i2c_bus, 0x27);
    let lcd_config = lcd::Config::default()
        .set_data_width(DataWidth::Bit4)
        .set_ram_type(RAMType::DDRam)
        .set_font(Font::Font5x8);

    // init LCD1602
    let mut lcd = Lcd::new(&mut sender, &mut delay, lcd_config, None);
    lcd.set_poll_interval(750000);
    lcd.clean_display();
    lcd.set_cursor_blink_state(State::Off);
    lcd.set_cursor_state(State::Off);
    lcd.set_display_state(State::Off);
    lcd.set_display_state(State::On);

    lcd.return_home();

    write_custom_chars_to_cgram(&mut lcd); // Load custom chars
    update_deck_display( &mut state, &mut lcd);

    let mut button_led: Pin<Output, arduino_hal::port::D2> = pins.d2.into_output(); // Microphone button LED
    let button_3 = pins.d3.into_pull_up_input(); // Microphone button

    let button_4 = pins.d4.into_pull_up_input();
    let button_5 = pins.d5.into_pull_up_input();
    let button_6 = pins.d6.into_pull_up_input();
    let button_7 = pins.d7.into_pull_up_input();
    let button_8 = pins.d8.into_pull_up_input();
    let button_9 = pins.d9.into_pull_up_input();

    //Debounce states
    let mut is_mic_button_pressed = false;

    let mut is_button_4_pressed = false;
    let mut is_button_5_pressed = false;
    let mut is_button_6_pressed = false;
    let mut is_button_7_pressed = false;
    let mut is_button_8_pressed = false;
    let mut is_button_9_pressed = false;

    led.toggle();

    let mut read_buffer: Vec<u8, 128> = Vec::<_, 128>::new();
    let mut is_transmiting = false;

    loop {
        match serial.read() {
            Ok(byte) => {
                match byte {
                    OPEN_TRANSITION_BYTE => {
                        is_transmiting = true;
                    }
                    CLOSE_TRANSITION_BYTE => {
                        is_transmiting = false;
                    }
                    _ => (),
                }

                if is_transmiting {
                    if read_buffer.push(byte).is_err() {}
                }
            }
            Err(_) => {
            }
        }

        //Protocol interpretation
        if !is_transmiting && read_buffer.len() > 0 {
            let mut i = 0;

            for byte in &read_buffer {
                match byte {
                    &MICROPHONE_BYTE=>{
                        let next_byte = read_buffer[i+1];

                        if next_byte == 48{
                            state.set_micro(false);
                        } else if next_byte == 49 {
                            state.set_micro(true);
                        }
                    }

                    &LIVE_BYTE=>{
                        let next_byte = read_buffer[i+1];

                        if next_byte == 48{
                            state.set_live(false);
                        } else if next_byte == 49 {
                            state.set_live(true);
                        }
                    }

                    &FOLLOWERS_BYTE=>{
                    let _ = string_buffer.push(read_buffer[i+1].as_ascii().unwrap().to_char());
                    let _ = string_buffer.push(read_buffer[i+2].as_ascii().unwrap().to_char());
                    let _ = string_buffer.push(read_buffer[i+3].as_ascii().unwrap().to_char());

                    state.set_followers(&string_buffer);
                    string_buffer = String::<3>::new();
                    }

                    &VIEWERS_BYTE => {
                        let _ = string_buffer.push(read_buffer[i+1].as_ascii().unwrap().to_char());
                        let _ = string_buffer.push(read_buffer[i+2].as_ascii().unwrap().to_char());
                        let _ = string_buffer.push(read_buffer[i+3].as_ascii().unwrap().to_char());

                        state.set_viewers(&string_buffer);
                        string_buffer = String::<3>::new();
                    }
                    _ => (),
                }
                i = i+1;
            }
            read_buffer.clear();
            i = 0;
        }


        let mic_button_state = button_3.is_low();

      if is_mic_button_pressed != mic_button_state{
            serial.write_str(if mic_button_state{"B3:1"} else {"B3:0"}).unwrap();
            serial.flush();
            // is_mic_button_pressed = true;
        };

        //Mic button
        if mic_button_state {
            is_mic_button_pressed = true;
        } else {
            is_mic_button_pressed = false;
            // serial.write_str("B3:0").unwrap();
        }

        if button_4.is_low() {
            if !is_button_4_pressed {
                serial.write_str("B4").unwrap();
                serial.flush();
                is_button_4_pressed = true;
            }
        } else {
            is_button_4_pressed = false;
        }

        if button_5.is_low() {
            if !is_button_5_pressed {
                serial.write_str("B5").unwrap();
                serial.flush();
                is_button_5_pressed = true;
            }
        } else {
            is_button_5_pressed = false;
        }

        if button_6.is_low() {
            if !is_button_6_pressed {
                serial.write_str("B6").unwrap();
                serial.flush();
                is_button_6_pressed = true;
            }
        } else {
            is_button_6_pressed = false;
        }

        if button_7.is_low() {
            if !is_button_7_pressed {
                serial.write_str("B7").unwrap();
                serial.flush();
                is_button_7_pressed = true;
            }
        } else {
            is_button_7_pressed = false;
        }

        if button_8.is_low() {
            if !is_button_8_pressed {
                serial.write_str("B8").unwrap();
                serial.flush();
                is_button_8_pressed = true;
            }
        } else {
            is_button_8_pressed = false;
        }

        if button_9.is_low() {
            if !is_button_9_pressed {
                serial.write_str("B9").unwrap();
                serial.flush();
                is_button_9_pressed = true;
            }
        } else {
            is_button_9_pressed = false;
        }


        update_deck_display(&mut state, &mut lcd);
        set_button_led(state.is_micro_on, &mut button_led);
    }
}
