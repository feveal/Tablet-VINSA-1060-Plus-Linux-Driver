// VINSA 1060 Plus Linux Driver - src/virtual_device.rs
use std::io::Error;
use std::collections::HashMap;
use gettextrs::gettext;
use xkbcommon::xkb;

use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AbsInfo, AbsoluteAxisType, RelativeAxisType, AttributeSet, EventType, InputEvent, Key, Synchronization,
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

    pub fn x_axis(&self) -> i32 {
        let raw = self.u16_from_2_u8(self.data[Self::X_AXIS_HIGH], self.data[Self::X_AXIS_LOW]);
        raw as i32
    }

    pub fn y_axis(&self) -> i32 {
        let raw = self.u16_from_2_u8(self.data[Self::Y_AXIS_HIGH], self.data[Self::Y_AXIS_LOW]);
        raw as i32
    }

    pub fn pressure(&self) -> i32 {
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
    _pen_button_id_to_key_code_map: HashMap<u8, Vec<Key>>,

    // Tres dispositivos independientes para no confundir a Libinput/Wayland
    virtual_pen: VirtualDevice,
    virtual_mouse: VirtualDevice,
    virtual_keyboard: VirtualDevice,

    was_touching: bool,
    pub is_mouse_mode: bool,
    last_x: i32,
    last_y: i32,
    last_valid_x: i32,
    mouse_area_scale: f32,

    // Historial para control estricto de duplicados USB
    last_raw_usb_x: i32,
    last_raw_usb_y: i32,
    last_raw_usb_pressure: i32,
    active_multimedia_key: Option<Key>,
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
            // --- FILA 1 ---
            (9, vec![Key::KEY_E]),          // "E" -> Borrador
            (12, vec![Key::KEY_B]),         // "B" -> Conmutar Modo Tablet/Mouse

            // --- FILA 2 ---
            (7, vec![Key::KEY_LEFTCTRL, Key::KEY_KPMINUS]), // Zoom -
            (8, vec![Key::KEY_LEFTCTRL, Key::KEY_KPPLUS]),  // Zoom +

            // --- FILA 3 ---
            (6, vec![Key::KEY_LEFTBRACE]),  // Tamaño Pincel -
            (13, vec![Key::KEY_RIGHTBRACE]), // Tamaño Pincel +

            // --- FILA 4 ---
            (5, vec![Key::KEY_PAGEUP]),     // Scroll Arriba
            (0, vec![Key::KEY_TAB]),        // TAB (Ocultar UI)

            // --- FILA 5 ---
            (4, vec![Key::KEY_PAGEDOWN]),   // Scroll Abajo
            (1, vec![Key::KEY_SPACE]),      // SPACE (Mover lienzo)

            // --- FILA 6 ---
            (3, vec![Key::KEY_LEFTCTRL]),   // CTRL
            (2, vec![Key::KEY_LEFTALT]),    // ALT
        ]
        .iter()
        .cloned()
        .collect();

        let default_pen_button_id_to_key_code_map: HashMap<u8, Vec<Key>> =
        [(4, vec![Key::BTN_STYLUS]), (6, vec![Key::BTN_STYLUS2])]
        .iter()
        .cloned()
        .collect();

        let mut all_keyboard_keys = AttributeSet::<Key>::new();
        for keys in default_tablet_button_id_to_key_code_map.values() {
            for key in keys {
                all_keyboard_keys.insert(*key);
            }
        }

        // Registrar preventivamente los códigos físicos
        all_keyboard_keys.insert(Key::KEY_APOSTROPHE);
        all_keyboard_keys.insert(Key::KEY_BACKSLASH);
        all_keyboard_keys.insert(Key::KEY_RIGHTALT);

        all_keyboard_keys.insert(Key::KEY_MUTE);          // 0. Mute
        all_keyboard_keys.insert(Key::KEY_VOLUMEDOWN);    // 1. Volumen -
        all_keyboard_keys.insert(Key::KEY_VOLUMEUP);      // 2. Volumen +
        all_keyboard_keys.insert(Key::KEY_PLAYER);        // 3. Música
        all_keyboard_keys.insert(Key::KEY_PLAYPAUSE);     // 4. Play / Pausa
        all_keyboard_keys.insert(Key::KEY_PREVIOUSSONG);  // 5. Saltar atrás
        all_keyboard_keys.insert(Key::KEY_NEXTSONG);      // 6. Saltar adelante
        all_keyboard_keys.insert(Key::KEY_HOMEPAGE);      // 7. Inicio (Navegador)
        all_keyboard_keys.insert(Key::KEY_CALC);          // 8. Calculadora
        all_keyboard_keys.insert(Key::KEY_STOPCD);        // 9. Stop

        DeviceDispatcher {
            tablet_last_raw_pressed_buttons: 0xFFFF,
            pen_last_raw_pressed_button: 0,
            tablet_button_id_to_key_code_map: default_tablet_button_id_to_key_code_map.clone(),
            _pen_button_id_to_key_code_map: default_pen_button_id_to_key_code_map.clone(),

            virtual_keyboard: Self::virtual_keyboard_builder(&all_keyboard_keys).expect("Error virtual_keyboard"),
            virtual_mouse: Self::virtual_mouse_builder().expect("Error virtual_mouse"),
            virtual_pen: Self::virtual_pen_builder().expect("Error virtual_pen"),
            was_touching: false,

            is_mouse_mode: true,
            last_x: 2048,
            last_y: 2048,

            mouse_area_scale: 0.10, // Valor de sensibilidad de inicio

            last_valid_x: 2048,
            last_raw_usb_x: -1,
            last_raw_usb_y: -1,
            last_raw_usb_pressure: -1,
            active_multimedia_key: None,
        }
    }




    pub fn syn(&mut self) -> Result<(), Error> {
        let _ = self.virtual_keyboard.emit(&[InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)]);
        let _ = self.virtual_mouse.emit(&[InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)]);
        let _ = self.virtual_pen.emit(&[InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)]);
        Ok(())
    }

    pub fn dispatch(&mut self, raw_data: &RawDataReader) {
        let current_raw_x = raw_data.x_axis();
        let current_raw_y = raw_data.y_axis();
        let current_raw_pressure = raw_data.pressure();

        // --- FILTRO DE DESCARTE ABSOLUTO ---
        if current_raw_x == self.last_raw_usb_x
            && current_raw_y == self.last_raw_usb_y
            && current_raw_pressure == self.last_raw_usb_pressure
            {
                return;
            }

            self.last_raw_usb_x = current_raw_x;
        self.last_raw_usb_y = current_raw_y;
        self.last_raw_usb_pressure = current_raw_pressure;

        self.emit_pen_events(raw_data);
        self.emit_tablet_events(raw_data);
    }

    fn emit_tablet_events(&mut self, raw_data: &RawDataReader) {
        let raw_button_as_binary_flags = raw_data.tablet_buttons_as_binary_flags();
        if raw_button_as_binary_flags != self.tablet_last_raw_pressed_buttons {
            self.binary_flags_to_tablet_key_events(raw_button_as_binary_flags);
            self.tablet_last_raw_pressed_buttons = raw_button_as_binary_flags;
        }
    }

    fn binary_flags_to_tablet_key_events(&mut self, raw_button_as_flags: u16) {
        // Procesar todos los IDs del 0 al 13 de forma consecutiva
        (0..14).for_each(|i| self.emit_tablet_key_event(i, raw_button_as_flags));
    }


    // Función para comprobar explícitamente el estado del modo tableta
    pub fn is_tablet_mode(&self) -> bool {
        !self.is_mouse_mode
    }

    pub fn emit_tablet_key_event(&mut self, i: u8, raw_button_as_flags: u16) {
        let id_as_binary_mask = 1 << i;
        let is_pressed = (raw_button_as_flags & id_as_binary_mask) == 0;
        let was_pressed = (self.tablet_last_raw_pressed_buttons & id_as_binary_mask) == 0;

        if let Some(state) = match (was_pressed, is_pressed) {
            (false, true) => Some(Self::PRESSED),
            (true, false) => Some(Self::RELEASED),
            _ => None,
        } {
            // 1. Control de modo universal (ID 12)
            if i == 12 {
                if state == Self::PRESSED {
                    self.is_mouse_mode = !self.is_mouse_mode;
                    println!(
                        "{}",
                        gettext(if self.is_mouse_mode {
                            "Switched mode to: MOUSE"
                        } else {
                            "Switched mode to: TABLET"
                        })
                    );
                }
                return;
            }

            // 2. Control de sensibilidad y combinación Alt Gr condicional según idioma (IDs 6 y 13)
            if i == 6 || i == 13 {
                if self.is_mouse_mode {
                    if state == Self::PRESSED {
                        if i == 6 {
                            // Baja de 2% en 2% hasta un mínimo de 0.04 (4%)
                            self.mouse_area_scale = (self.mouse_area_scale - 0.02).max(0.04);
                        } else {
                            // Sube de 2% en 2% hasta un máximo de 0.30 (30%)
                            self.mouse_area_scale = (self.mouse_area_scale + 0.02).min(0.30);
                        }
                        println!(
                            "{} {:.0}%",
                            gettext("Adjusted mouse speed:"),
                            self.mouse_area_scale * 100.0
                        );
                    }
                } else if self.is_tablet_mode() {
                    // DETECCIÓN DINÁMICA DE IDIOMA
                    let es_espanol = Self::es_layout_espanol();


                    // Si es español, presionamos Alt Gr (KEY_RIGHTALT) antes de enviar el corchete
                    if es_espanol {
                        let _ = self.virtual_keyboard.emit(&[
                            InputEvent::new(EventType::KEY, Key::KEY_RIGHTALT.code(), state)
                        ]);
                    }

                    // Enviamos la tecla del corchete correspondiente (ID 6 -> [ , ID 13 -> ])
                    let key_leftbr = if i == 6 { Key::KEY_LEFTBRACE } else { Key::KEY_RIGHTBRACE };
                    let _ = self.virtual_keyboard.emit(&[
                        InputEvent::new(EventType::KEY, key_leftbr.code(), state)
                    ]);

                    // Sincronizamos la combinación en Wayland / Libinput
                    let _ = self.virtual_keyboard.emit(&[
                        InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)
                    ]);
                }
                return;
            }

            // 3. Emisión universal estándar para el resto de botones (Fila 2, 4, 5 y 6)
            if let Some(keys) = self.tablet_button_id_to_key_code_map.get(&i) {
                if i == 12 {
                    return;
                }

                for &key in keys {
                    let _ = self.virtual_keyboard.emit(&[
                        InputEvent::new(EventType::KEY, key.code(), state)
                    ]);
                }
                let _ = self.virtual_keyboard.emit(&[
                    InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)
                ]);
            }
        }
    }

    fn virtual_keyboard_builder(key_set: &AttributeSet<Key>) -> Result<VirtualDevice, Error> {
        VirtualDeviceBuilder::new()?
            .name("VINSA Pad Buttons")
            .with_keys(key_set)? // Mapeo dinámico
            .build()
    }

    fn virtual_mouse_builder() -> Result<VirtualDevice, Error> {
        let mut key_set = AttributeSet::<Key>::new();
        key_set.insert(Key::BTN_LEFT);
        key_set.insert(Key::BTN_RIGHT);
        key_set.insert(Key::BTN_MIDDLE);

        let mut rel_set = AttributeSet::<RelativeAxisType>::new();
        rel_set.insert(RelativeAxisType::REL_X);
        rel_set.insert(RelativeAxisType::REL_Y);

        VirtualDeviceBuilder::new()?
        .name("VINSA Mouse Emulation")
        .with_relative_axes(&rel_set)?
        .with_keys(&key_set)?
        .build()
    }

    fn virtual_pen_builder() -> Result<VirtualDevice, Error> {
        let abs_x_setup = UinputAbsSetup::new(AbsoluteAxisType::ABS_X, AbsInfo::new(0, 0, 4096, 0, 0, 1));
        let abs_y_setup = UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, AbsInfo::new(0, 0, 4096, 0, 0, 1));
        let abs_pressure_setup = UinputAbsSetup::new(AbsoluteAxisType::ABS_PRESSURE, AbsInfo::new(0, 0, 8191, 0, 0, 1));

        let mut key_set = AttributeSet::<Key>::new();
        let pen_keys = &[Key::BTN_TOOL_PEN, Key::BTN_TOUCH, Key::BTN_STYLUS, Key::BTN_STYLUS2];
        for key in pen_keys {
            key_set.insert(*key);
        }

        VirtualDeviceBuilder::new()?
        // NOMBRE FORMATEADO PARA REGLAS GENERALES DE WACOM EN LIBINPUT
        // Al incluir "Wacom" y terminar estrictamente en "Pen", libinput activa la compatibilidad de Stylus integrada de forma agnóstica.
        .name("Wacom HID 0357 Pen")
        .with_absolute_axis(&abs_x_setup)?
        .with_absolute_axis(&abs_y_setup)?
        .with_absolute_axis(&abs_pressure_setup)?
        .with_keys(&key_set)?
        .build()
    }

    fn normalize_pressure_mode(raw_pressure: i32, threshold: i32, scaling: i32) -> i32 {
        let inverted_pressure = 1300 - raw_pressure;
        if inverted_pressure <= threshold { 0 } else { ((inverted_pressure - threshold) * scaling).clamp(0, 8191) }
    }


    fn emit_pen_events(&mut self, raw_data: &RawDataReader) {
        let y_raw = raw_data.y_axis();
        let is_multimedia_area = y_raw >= 61000;

        // --- INTERCEPCIÓN TOTAL DEL ÁREA MULTIMEDIA ---
        if is_multimedia_area {
            // Forzamos la liberación inmediata del touch/click por si veníamos arrastrando el lápiz desde abajo
            if self.was_touching {
                self.pen_emit_touch(0);
            }

            // Procesamos la pulsación del botón multimedia basado en la presión invertida (< 1200)
            self.process_multimedia_area(raw_data.x_axis(), raw_data.pressure());

            // Con return, evitamos por completo enviar coordenadas absolutas o eventos de click
            return;
        } else {
            // Si salimos de la zona multimedia, nos aseguramos de liberar cualquier botón enganchado
            if self.active_multimedia_key.is_some() {
                self.process_multimedia_area(raw_data.x_axis(), 1300); // 1300 = Aire (libera)
            }
            self.last_valid_x = raw_data.x_axis();
        }

        // --- FLUJO NORMAL FUERA DEL ÁREA MULTIMEDIA ---
        let raw_pen_buttons = raw_data.pen_buttons();
        if raw_pen_buttons != self.pen_last_raw_pressed_button {
            self.raw_pen_buttons_to_pen_key_events(raw_pen_buttons);
            self.pen_last_raw_pressed_button = raw_pen_buttons;
        }

        let normalized_pressure = if self.is_mouse_mode {
            Self::normalize_pressure_mode(raw_data.pressure(), 300, 10)
        } else {
            Self::normalize_pressure_mode(raw_data.pressure(), 20, 12)
        };

        // Aquí es donde se mueven y hacen click los dispositivos virtuales de evdev
        self.raw_pen_abs_to_pen_abs_events(raw_data.x_axis(), raw_data.y_axis(), normalized_pressure, is_multimedia_area);
        self.pen_emit_touch(normalized_pressure);
    }

    fn raw_pen_abs_to_pen_abs_events(&mut self, x_axis: i32, y_axis: i32, pressure: i32, is_multimedia_area: bool) {
        if self.is_mouse_mode {
            // --- MODO RATÓN RELATIVO DINÁMICO ---
            if self.last_x != 2048 && !is_multimedia_area {

                let rel_x = ((x_axis - self.last_x) as f32 * self.mouse_area_scale * 12.0).round() as i32;
                let rel_y = ((y_axis - self.last_y) as f32 * self.mouse_area_scale * 12.0).round() as i32;

                if rel_x != 0 || rel_y != 0 {
                    let _ = self.virtual_mouse.emit(&[
                        InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, rel_x),
                                                    InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, rel_y),
                    ]);
                }
            }
            self.last_x = x_axis;
            self.last_y = y_axis;
        } else {

            // --- MODO TABLETA ABSOLUTO INTERPOLADO (PARA KRITA) ---
            let tx = x_axis;
            let ty = y_axis.clamp(0, 4095);

            // CORRECCIÓN DEL FILTRO
            if tx == self.last_raw_usb_x && ty == self.last_raw_usb_y && pressure == self.last_raw_usb_pressure {
                return;
            }

            let dx = tx - self.last_x;
            let dy = ty - self.last_y;
            let max_gap = dx.abs().max(dy.abs());
            let last_press = if self.last_raw_usb_pressure == -1 { pressure } else { self.last_raw_usb_pressure };
            let d_press = pressure - last_press;

            let allowed_step = 8;
            if max_gap > allowed_step && self.last_x != 2048 && !is_multimedia_area && pressure > 0 {
                let steps = (max_gap / allowed_step).clamp(2, 6);
                for s in 1..steps {
                    let t = s as f32 / steps as f32;
                    let interp_x = (self.last_x as f32 + (dx as f32 * t)).round() as i32;
                    let interp_y = (self.last_y as f32 + (dy as f32 * t)).round() as i32;
                    let interp_press = (last_press as f32 + (d_press as f32 * t)).round() as i32;

                    let _ = self.virtual_pen.emit(&[
                        InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, interp_x),
                                                  InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, interp_y),
                                                  InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_PRESSURE.0, interp_press),
                                                  InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)
                    ]);
                }
            }

            let _ = self.virtual_pen.emit(&[
                InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, tx),
                                          InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, ty),
                                          InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_PRESSURE.0, pressure),
            ]);

            // Guardar el estado usando los nombres correctos
            self.last_raw_usb_x = tx;
            self.last_raw_usb_y = ty;
            self.last_raw_usb_pressure = pressure;
            self.last_x = tx;
            self.last_y = ty;
        }
    }

    fn pen_emit_touch(&mut self, pressure: i32) {
        let is_touching = pressure > 0;

        if !self.is_mouse_mode {
            // --- CORRECCIÓN PARA EL MODO TABLETA EN WAYLAND ---
            // Mientras el driver reciba datos de la tableta, el lápiz está en rango.
            // Forzamos BTN_TOOL_PEN a 1 para que Kubuntu mantenga el cursor activo en el aire.
            let _ = self.virtual_pen.emit(&[
                InputEvent::new(EventType::KEY, Key::BTN_TOOL_PEN.code(), 1),
            ]);
        }

        if let Some(state) = match (self.was_touching, is_touching) {
            (false, true) => Some(Self::PRESSED),
            (true, false) => Some(Self::RELEASED),
            _ => None,
        } {
            if self.is_mouse_mode {
                // En modo ratón el contacto físico equivale al clic izquierdo normal
                let _ = self.virtual_mouse.emit(&[
                    InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), state),
                ]);
            } else {
                // En modo tableta, el contacto físico equivale al BTN_TOUCH de Wacom
                let _ = self.virtual_pen.emit(&[
                    InputEvent::new(EventType::KEY, Key::BTN_TOUCH.code(), state),
                ]);
            }
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
            if self.is_mouse_mode {
                let target_click = if id == 6 { Key::BTN_RIGHT } else { Key::BTN_MIDDLE };
                let _ = self.virtual_mouse.emit(&[InputEvent::new(EventType::KEY, target_click.code(), state)]);
            } else {
                if let Some(keys) = self.tablet_button_id_to_key_code_map.get(&id) {
                    for key in keys {
                        let _ = self.virtual_pen.emit(&[InputEvent::new(EventType::KEY, key.code(), state)]);
                    }
                }
            }
        }
    }

    fn es_layout_espanol() -> bool {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        if let Some(keymap) = xkb::Keymap::new_from_names(
            &ctx,
            "", "", "", "",
            None::<String>,
            xkb::KEYMAP_COMPILE_NO_FLAGS
        ) {
            let layout_name = keymap.layout_get_name(0).to_lowercase();

            return layout_name.contains("es") || layout_name.contains("span");
        }
        false
    }


    fn process_multimedia_area(&mut self, x: i32, pressure: i32) {

        let is_touching = pressure < 1200;

        if is_touching {
            if self.active_multimedia_key.is_some() {
                return;
            }

            let index = (x / 410).clamp(0, 9);

            let target_key = match index {
                0 => Key::KEY_MUTE,
                1 => Key::KEY_VOLUMEDOWN,
                2 => Key::KEY_VOLUMEUP,
                3 => Key::KEY_PLAYER,
                4 => Key::KEY_PLAYPAUSE,
                5 => Key::KEY_PREVIOUSSONG,
                6 => Key::KEY_NEXTSONG,
                7 => Key::KEY_HOMEPAGE,
                8 => Key::KEY_CALC,
                _ => Key::KEY_STOPCD,
            };

            let _ = self.virtual_keyboard.emit(&[
                InputEvent::new(EventType::KEY, target_key.code(), Self::PRESSED)
            ]);
            let _ = self.virtual_keyboard.emit(&[
                InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)
            ]);

            self.active_multimedia_key = Some(target_key);
        } else {
            if let Some(key) = self.active_multimedia_key.take() {
                let _ = self.virtual_keyboard.emit(&[
                    InputEvent::new(EventType::KEY, key.code(), Self::RELEASED)
                ]);
                let _ = self.virtual_keyboard.emit(&[
                    InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0)
                ]);
            }
        }
    }
}
