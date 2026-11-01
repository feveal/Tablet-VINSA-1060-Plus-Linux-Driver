// VINSA 1060 Plus Linux Driver - src/main.rs (by feveal@hotmail.com)
mod virtual_device;
mod physical_device;

use std::u16;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use physical_device::PhysicalDevice;
use virtual_device::{DeviceDispatcher, RawDataReader};
use gettextrs::{
    bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain, LocaleCategory,
};

// Dependencias para la barra de tareas y bucle de eventos
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};
use winit::event_loop::{ControlFlow, EventLoop};

const VID: u16 = 0x08f2;
const PID: u16 = 0x6811;
const TEXT_DOMAIN: &str = "v1060p";

fn locale_catalog_languages(locale: &str) -> Vec<String> {
    locale
        .split(':')
        .filter_map(|language| {
            let language = language.split(['.', '@']).next().unwrap_or(language);
            if language.is_empty()
                || language == "C"
                || language == "POSIX"
                || !language
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                return None;
            }

            Some(language.to_owned())
        })
        .flat_map(|language| {
            let base_language = language.split('_').next().unwrap_or(&language).to_owned();
            if base_language == language {
                vec![language]
            } else {
                vec![language, base_language]
            }
        })
        .collect()
}

fn init_i18n() -> Result<(), Box<dyn std::error::Error>> {
    let locale = unsafe { setlocale(LocaleCategory::LcMessages, "") };
    let locale = locale.ok_or("Could not initialize the system message locale")?;
    let locale = String::from_utf8_lossy(&locale);
    let language_preference = std::env::var("LANGUAGE").unwrap_or_else(|_| locale.into_owned());
    let catalog_languages = locale_catalog_languages(&language_preference);

    let mut locale_roots = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            locale_roots.push(parent.join("locale"));
        }
    }
    locale_roots.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/locale"
    )));
    locale_roots.push(PathBuf::from("/usr/local/share/locale"));
    locale_roots.push(PathBuf::from("/usr/share/locale"));

    let locale_root = locale_roots
        .into_iter()
        .find(|root| {
            catalog_languages
                .iter()
                .any(|language| root.join(language).join("LC_MESSAGES/v1060p.mo").is_file())
        })
        .unwrap_or_else(|| PathBuf::from("/usr/share/locale"));

    bindtextdomain(TEXT_DOMAIN, locale_root)?;
    bind_textdomain_codeset(TEXT_DOMAIN, "UTF-8")?;
    textdomain(TEXT_DOMAIN)?;
    Ok(())
}

fn translated_format(message: &str, value: impl Display) -> String {
    gettext(message).replacen("{}", &value.to_string(), 1)
}


// Función auxiliar para parsear e incrustar el SVG en la barra de tareas
fn crear_icono_desde_svg_bytes(bytes: &[u8], tamano: u32) -> Result<Icon, String> {
    // 1. Configurar las opciones de parseo del SVG
    let opt = usvg::Options::default();

    // 2. Parsear el árbol XML del SVG desde los bytes en memoria
    let tree = usvg::Tree::from_data(bytes, &opt)
        .map_err(|e| translated_format("Error parsing the SVG file: {}", e))?;

    // 3. Crear un mapa de píxeles (pixmap) vacío con el tamaño objetivo
    let mut pixmap = resvg::tiny_skia::Pixmap::new(tamano, tamano)
        .ok_or_else(|| gettext("Could not create the pixel buffer in memory"))?;

    // 4. Renderizar el vector SVG sobre el mapa de píxeles usando un escalado automático
    let transform = resvg::tiny_skia::Transform::from_scale(
        tamano as f32 / tree.size().width(),
        tamano as f32 / tree.size().height(),
    );

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Extraer los datos RGBA planos
    let rgba = pixmap.data().to_vec();

    // 5. Construir la estructura final que espera tray-icon
    Icon::from_rgba(rgba, tamano, tamano)
        .map_err(|e| translated_format("Error generating the icon structure: {}", e))
}

fn main() {
    if let Err(e) = init_i18n() {
        eprintln!(
            "{}",
            translated_format("Could not initialize translations: {}", e)
        );
    }

    // Inicializar GTK para evitar el pánico del hilo principal
    if let Err(e) = gtk::init() {
        eprintln!("{}", translated_format("Error initializing GTK: {}", e));
        std::process::exit(1);
    }

    // Incrustar los archivos SVG en tiempo de compilación
    let bytes_raton = include_bytes!("../resources/raton.svg");
    let bytes_tableta = include_bytes!("../resources/tableta-grafica.svg");

    // Rasterizar ambos iconos vectoriales a un tamaño nítido de 24x24 píxeles
    let icono_raton = crear_icono_desde_svg_bytes(bytes_raton, 24)
        .unwrap_or_else(|e| {
            eprintln!(
                "{}",
                translated_format("Critical error loading SVG for mouse: {}, using fallback.", e)
            );
            Icon::from_rgba(vec![0; 4], 1, 1).unwrap()
        });

    let icono_tableta = crear_icono_desde_svg_bytes(bytes_tableta, 24)
        .unwrap_or_else(|e| {
            eprintln!(
                "{}",
                translated_format(
                    "Critical error loading SVG for tablet: {}, using fallback.",
                    e
                )
            );
            Icon::from_rgba(vec![0; 4], 1, 1).unwrap()
        });

    // 1. Inicialización de dispositivos físicos
    let mut physical_device = PhysicalDevice::new(VID, PID);
    physical_device.init().set_full_mode();

    // Compartir el dispatcher en un Mutex seguro entre hilos
    let dispatcher = Arc::new(Mutex::new(DeviceDispatcher::new()));
    let dispatcher_clone = Arc::clone(&dispatcher);

    // 2. HILO SECUNDARIO: Lector USB en segundo plano
    thread::spawn(move || {
        let mut data_reader = RawDataReader::new();
        loop {
            let mut read_at_least_one = false;

            while let Ok(_) = physical_device.read_device_responses(&mut data_reader.data) {
                if let Ok(mut disp) = dispatcher_clone.lock() {
                    disp.dispatch(&data_reader);
                }
                read_at_least_one = true;
            }

            if read_at_least_one {
                if let Ok(mut disp) = dispatcher_clone.lock() {
                    let _ = disp.syn();
                }
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    });

    // 3. HILO PRINCIPAL: Bucle de eventos UI y Gestión del System Tray
    let event_loop = EventLoop::new().unwrap();

    // Crear el menú contextual (Click derecho sobre el icono)
    let tray_menu = Menu::new();
    let quit_item = MenuItem::new(gettext("Quit"), true, None);
    let status_item = MenuItem::new(gettext("Mode: Mouse"), false, None);

    tray_menu.append(&status_item).unwrap();
    tray_menu.append(&PredefinedMenuItem::separator()).unwrap();
    tray_menu.append(&quit_item).unwrap();

    let icono_inicial = icono_raton.clone();

    let mut tray_icon = Some(
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Driver VINSA 1060 Plus")
            .with_icon(icono_inicial)
            .build()
            .unwrap(),
    );

    // Escuchador de eventos del menú de la bandeja
    let menu_channel = MenuEvent::receiver();

    let dispatcher_ui = Arc::clone(&dispatcher);
    let mut last_known_mode = true; // true = Ratón, false = Tableta

    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(100)
    ));

    event_loop.run(move |_event, event_loop| {
        // Ejecutar iteración del bucle principal de GTK para procesar eventos
        gtk::main_iteration_do(false);

        // Monitorear si el driver cambió de modo de forma interna
        if let Ok(disp) = dispatcher_ui.lock() {
            if disp.is_mouse_mode != last_known_mode {
                last_known_mode = disp.is_mouse_mode;

                if let Some(ref mut ti) = tray_icon {
                    if last_known_mode {
                        status_item.set_text(gettext("Mode: Mouse"));
                        let _ = ti.set_icon(Some(icono_raton.clone()));
                    } else {
                        status_item.set_text(gettext("Mode: Tablet"));
                        let _ = ti.set_icon(Some(icono_tableta.clone()));
                    }
                }
            }
        }

        // Procesar clics en el menú contextual
        if let Ok(event) = menu_channel.try_recv() {
            if event.id == quit_item.id() {
                drop(tray_icon.take());
                event_loop.exit();
            }
        }
    }).unwrap();
}
