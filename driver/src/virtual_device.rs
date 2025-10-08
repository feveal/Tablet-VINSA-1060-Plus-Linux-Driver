// VINSA 1060 Plus Linux Driver (V2), (by feveal@hotmail.com)
use std::io::Error;
use std::collections::HashMap;

use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AbsInfo, AbsoluteAxisType, AttributeSet, EventType, InputEvent, Key, Synchronization,
    UinputAbsSetup,
};

#[derive(Default)]
pub struct RawDataReader {
    pub data: Vec<u8>,
}

impl RawDataReader {
    const X_AXIS_HIGH: usize = 1;
    const X_AXIS_LOW: usize = 2;
    const Y_AXIS_HIGH: usize = 3;
    const Y_AXIS_LOW: usize = 4;
    const PRESSURE_HIGH: usize = 5;
    const PRESSURE_LOW: usize = 6;
    const PEN_BUTTONS: usize = 9;
    const TABLET_BUTTONS_HIGH: usize = 12;
    const TABLET_BUTTONS_LOW: usize = 11;

    pub fn new() -> Self {
        RawDataReader {
            data: vec![0u8; 64],
        }
    }

    fn u16_from_2_u8(&self, high: u8, low: u8) -> u16 {
        (high as u16) << 8 | low as u16
    }

    fn x_axis(&self) -> i32 {
        let raw = self.u16_from_2_u8(self.data[Self::X_AXIS_HIGH], self.data[Self::X_AXIS_LOW]);
        raw as i32
    }

    fn y_axis(&self) -> i32 {
        let raw = self.u16_from_2_u8(self.data[Self::Y_AXIS_HIGH], self.data[Self::Y_AXIS_LOW]);
        raw as i32
    }

    fn pressure(&self) -> i32 {
        self.u16_from_2_u8(
            self.data[Self::PRESSURE_HIGH],
            self.data[Self::PRESSURE_LOW],
        ) as i32
    }

    fn tablet_buttons_as_binary_flags(&self) -> u16 {
        self.u16_from_2_u8(
            self.data[Self::TABLET_BUTTONS_HIGH],
            self.data[Self::TABLET_BUTTONS_LOW],
        ) | (0xcc << 8)
    }

    fn pen_buttons(&self) -> u8 {
        self.data[Self::PEN_BUTTONS]
    }
}

pub struct DeviceDispatcher {
    tablet_last_raw_pressed_buttons: u16,
    pen_last_raw_pressed_button: u8,
    tablet_button_id_to_key_code_map: HashMap<u8, Vec<Key>>,
    pen_button_id_to_key_code_map: HashMap<u8, Vec<Key>>,
    virtual_pen: VirtualDevice,
    virtual_keyboard: VirtualDevice,
    was_touching: bool,
    is_mouse_mode: bool,
    last_x: i32,
    last_y: i32,
    last_valid_x: i32,
    mouse_area_scale: f32,
}

impl Default for DeviceDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceDispatcher {
    const PRESSED: i32 = 1;
    const RELEASED: i32 = 0;
    const HOLD: i32 = 2;

    pub fn new() -> Self {
        let default_tablet_button_id_to_key_code_map: HashMap<u8, Vec<Key>> = [
            (0, vec![Key::KEY_TAB]),        // TAB
            (1, vec![Key::KEY_SPACE]),      // SPACE
            (2, vec![Key::KEY_LEFTALT]),    // ALT
            (3, vec![Key::KEY_LEFTCTRL]),   // CTRL
            (4, vec![Key::KEY_PAGEUP]),     // MOUSE UP
            (5, vec![Key::KEY_PAGEDOWN]),   // MOUSE DOWN
            (6, vec![Key::KEY_LEFTBRACE]),  // MOUSE AREA -
            (7, vec![Key::KEY_LEFTCTRL, Key::KEY_KPMINUS]), // CTRL- ZOOM
            (8, vec![Key::KEY_LEFTCTRL, Key::KEY_KPPLUS]),  // CTRL+ ZOOM
            (9, vec![Key::KEY_ESC]),        // ESC CANCEL
            (12, vec![Key::KEY_B]),         // TOGGLE MOUSE/TABLET
            (13, vec![Key::KEY_RIGHTBRACE]), // MOUSE AREA +
        ]
        .iter()
        .cloned()
        .collect();

        let default_pen_button_id_to_key_code_map: HashMap<u8, Vec<Key>> =
            [(4, vec![Key::BTN_STYLUS]), (6, vec![Key::BTN_STYLUS2])]
                .iter()
                .cloned()
                .collect();

        DeviceDispatcher {
            tablet_last_raw_pressed_buttons: 0xFFFF,
            pen_last_raw_pressed_button: 0,
            tablet_button_id_to_key_code_map: default_tablet_button_id_to_key_code_map.clone(),
            pen_button_id_to_key_code_map: default_pen_button_id_to_key_code_map.clone(),
            virtual_pen: Self::virtual_pen_builder(
                &default_pen_button_id_to_key_code_map
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<Key>>(),
            )
            .expect("Error building virtual pen"),
            virtual_keyboard: Self::virtual_keyboard_builder(
                &default_tablet_button_id_to_key_code_map
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<Key>>(),
            )
            .expect("Error building virtual keyboard"),
            was_touching: false,
            is_mouse_mode: true,
            last_x: 2048,
            last_y: 2048,
            mouse_area_scale: 0.3,
            last_valid_x: 2048,
        }
    }

    fn smooth_coordinates(&mut self, x: i32, y: i32) -> (i32, i32) {
        let (smoothed_x, smoothed_y) = if self.is_mouse_mode {
            ((self.last_x * 1 + x) / 2, (self.last_y * 1 + y) / 2)
        } else {
            ((self.last_x * 3 + x) / 4, (self.last_y * 3 + y) / 4)
        };

        self.last_x = smoothed_x;
        self.last_y = smoothed_y;

        (smoothed_x, smoothed_y)
    }

    pub fn syn(&mut self) -> Result<(), Error> {
        self.virtual_keyboard.emit(&[InputEvent::new(
            EventType::SYNCHRONIZATION,
            Synchronization::SYN_REPORT.0,
            0,
        )])?;
        self.virtual_pen.emit(&[InputEvent::new(
            EventType::SYNCHRONIZATION,
            Synchronization::SYN_REPORT.0,
            0,
        )])?;
        Ok(())
    }

    pub fn dispatch(&mut self, raw_data: &RawDataReader) {
        self.emit_pen_events(raw_data);
        self.emit_tablet_events(raw_data);
    }

    fn emit_tablet_events(&mut self, raw_data: &RawDataReader) {
        let raw_button_as_binary_flags = raw_data.tablet_buttons_as_binary_flags();
        self.binary_flags_to_tablet_key_events(raw_button_as_binary_flags);
        self.tablet_last_raw_pressed_buttons = raw_button_as_binary_flags;
    }

    fn virtual_keyboard_builder(tablet_emitted_keys: &[Key]) -> Result<VirtualDevice, Error> {
        let mut key_set = AttributeSet::<Key>::new();
        for key in tablet_emitted_keys {
            key_set.insert(*key);
        }

        VirtualDeviceBuilder::new()?
            .name("virtual_tablet")
            .with_keys(&key_set)?
            .build()
    }

    fn binary_flags_to_tablet_key_events(&mut self, raw_button_as_flags: u16) {
        (0..14)
            .filter(|i| ![10, 11].contains(i))
            .for_each(|i| self.emit_tablet_key_event(i, raw_button_as_flags));
    }

    pub fn emit_tablet_key_event(&mut self, i: u8, raw_button_as_flags: u16) {
        let id_as_binary_mask = 1 << i;
        let is_pressed = (raw_button_as_flags & id_as_binary_mask) == 0;
        let was_pressed = (self.tablet_last_raw_pressed_buttons & id_as_binary_mask) == 0;

        if let Some(state) = match (was_pressed, is_pressed) {
            (false, true) => Some(Self::PRESSED),
            (true, false) => Some(Self::RELEASED),
            (true, true) => Some(Self::HOLD),
            _ => None,
        } {
            // Button [ - Reduce mouse area
            if i == 6 && state == Self::PRESSED {
                self.mouse_area_scale = (self.mouse_area_scale * 0.8).max(0.1);
                eprintln!("Mouse area reduced: {:.0}%", self.mouse_area_scale * 100.0);
                return;
            }

            // Button ] - Enlarge mouse area
            if i == 13 && state == Self::PRESSED {
                self.mouse_area_scale = (self.mouse_area_scale * 1.2).min(0.4);
                eprintln!("Mouse area increased: {:.0}%", self.mouse_area_scale * 100.0);
                return;
            }

            // Toggle with B button
            if i == 12 && state == Self::PRESSED {
                self.is_mouse_mode = !self.is_mouse_mode;
                eprintln!("Mode: {}", if self.is_mouse_mode { "MOUSE" } else { "TABLET" });
                return;
            }

            if let Some(keys) = self.tablet_button_id_to_key_code_map.get(&i) {
                for &key in keys {
                    self.virtual_keyboard
                        .emit(&[InputEvent::new(EventType::KEY, key.code(), state)])
                        .expect("Error emitting virtual keyboard key.");
                }

                self.virtual_keyboard
                    .emit(&[InputEvent::new(
                        EventType::SYNCHRONIZATION,
                        Synchronization::SYN_REPORT.0,
                        0,
                    )])
                    .expect("Error emitting SYN.");
            }
        }
    }

    fn virtual_pen_builder(pen_emitted_keys: &[Key]) -> Result<VirtualDevice, Error> {
        let abs_x_setup =
            UinputAbsSetup::new(AbsoluteAxisType::ABS_X, AbsInfo::new(0, 0, 4096, 0, 0, 1));
        let abs_y_setup =
            UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, AbsInfo::new(0, 0, 4096, 0, 0, 1));
        let abs_pressure_setup = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_PRESSURE,
            AbsInfo::new(0, 0, 8191, 0, 0, 1), // Cambiado a 8191
        );

        let mut key_set = AttributeSet::<Key>::new();
        for key in pen_emitted_keys {
            key_set.insert(*key);
        }

        for key in &[Key::BTN_TOOL_PEN, Key::BTN_LEFT, Key::BTN_RIGHT] {
            key_set.insert(*key);
        }

        VirtualDeviceBuilder::new()?
            .name("virtual_tablet")
            .with_absolute_axis(&abs_x_setup)?
            .with_absolute_axis(&abs_y_setup)?
            .with_absolute_axis(&abs_pressure_setup)?
            .with_keys(&key_set)?
            .build()
    }

    fn emit_pen_events(&mut self, raw_data: &RawDataReader) {
        let y_raw = raw_data.y_axis();
        let is_multimedia_area = y_raw >= 61000;

        if !is_multimedia_area {
            self.last_valid_x = raw_data.x_axis();
        }

        let raw_pen_buttons = raw_data.pen_buttons();
        self.raw_pen_buttons_to_pen_key_events(raw_pen_buttons);
        self.pen_last_raw_pressed_button = raw_pen_buttons;

        // Pressure normalization by mode
        let normalized_pressure = if self.is_mouse_mode {
            Self::normalize_pressure_mode(raw_data.pressure(), 800, 2)
        } else {
            Self::normalize_pressure_mode(raw_data.pressure(), 510, 3)
        };

        let (smoothed_x, smoothed_y) = if is_multimedia_area {
            (self.last_valid_x, 0) // Multimedia area: last X, top Y
        } else {
            self.smooth_coordinates(raw_data.x_axis(), raw_data.y_axis())
        };

        self.raw_pen_abs_to_pen_abs_events(
            smoothed_x,
            smoothed_y,
            normalized_pressure,
            is_multimedia_area
        );

        self.pen_emit_touch(raw_data);
    }

    fn normalize_pressure_mode(raw_pressure: i32, threshold: i32, scaling: i32) -> i32 {
        match 2000 - raw_pressure {
            x if x <= threshold => 0,
            x => x * scaling,
        }
    }

    fn raw_pen_abs_to_pen_abs_events(&mut self, x_axis: i32, y_axis: i32, pressure: i32, is_multimedia_area: bool) {
        let (x, y) = if is_multimedia_area {
            (self.last_valid_x, 0) // Use last valid X and top position
        } else if self.is_mouse_mode {
            let center_x = 1024;
            let center_y = 2048;
            let range = (4096.0 * self.mouse_area_scale) as i32;
            let scale_factor = 4096 / range.max(1);

            let scaled_x = ((x_axis - center_x) * scale_factor) + 2048;
            let scaled_y = ((y_axis - center_y) * scale_factor) + 2048;

            (scaled_x.clamp(0, 4096), scaled_y.clamp(0, 4096))
        } else {
            (x_axis, y_axis.clamp(0, 4095))
        };

        self.virtual_pen.emit(&[InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_X.0,
            x,
        )]).expect("Error emitting ABS_X.");

        self.virtual_pen.emit(&[InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_Y.0,
            y,
        )]).expect("Error emitting ABS_Y.");

        self.virtual_pen.emit(&[InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_PRESSURE.0,
            pressure,
        )]).expect("Error emitting Pressure.");
    }

    fn pen_emit_touch(&mut self, raw_data: &RawDataReader) {
        let normalized_pressure = if self.is_mouse_mode {
            Self::normalize_pressure_mode(raw_data.pressure(), 800, 2)
        } else {
            Self::normalize_pressure_mode(raw_data.pressure(), 510, 3)
        };

        let is_touching = normalized_pressure > 0;
        if let Some(state) = match (self.was_touching, is_touching) {
            (false, true) => Some(Self::PRESSED),
            (true, false) => Some(Self::RELEASED),
            _ => None,
        } {
            self.virtual_pen.emit(&[InputEvent::new(
                EventType::KEY,
                Key::BTN_TOUCH.code(),
                state,
            )]).expect("Error emitting Touch");
        }
        self.was_touching = is_touching;
    }

    fn raw_pen_buttons_to_pen_key_events(&mut self, pen_button: u8) {
        if let Some((state, id)) = match (self.pen_last_raw_pressed_button, pen_button) {
            (2, x) if x == 6 || x == 4 => Some((Self::PRESSED, x)),
            (x, 2) if x == 6 || x == 4 => Some((Self::RELEASED, x)),
            (x, y) if x != 2 && x == y => Some((Self::HOLD, x)),
            _ => None,
        } {
            if let Some(keys) = self.pen_button_id_to_key_code_map.get(&id) {
                for key in keys {
                    self.virtual_pen
                        .emit(&[InputEvent::new(EventType::KEY, key.code(), state)])
                        .expect("Error emitting pen keys.")
                }
            }
        }
    }
}
