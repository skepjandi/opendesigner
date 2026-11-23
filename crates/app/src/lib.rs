//! OpenDesigner editor: a Qt Quick window over the Rust core. The binary is a thin `main` over
//! [`run`]; the code lives in this library so the tests link the generated Qt glue.

pub mod asset_packs;
pub mod canvas;
pub mod chat;
pub mod editor;
mod frame_bridge;
pub mod layout;
pub mod preview;
pub mod servers;
pub mod showcase;
pub mod sound_rows;
pub mod state;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

const MAIN_QML: &str = "qrc:/qt/qml/org/opendesigner/app/qml/main.qml";

unsafe extern "C" {
    fn od_init_web_engine();
}

/// Opens the editor window and runs the event loop. Returns the process exit code.
pub fn run() -> i32 {
    // The Qt glue of each crate registers itself through an initializer. Linux links those in
    // whole, so the program must say which ones it uses; macOS only keeps what is referenced.
    cxx_qt::init_crate!(cxx_qt);
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_crate!(opendesigner_app);
    cxx_qt::init_qml_module!("org.opendesigner.app");
    // SAFETY: a plain C function with no arguments, defined in `cpp/web_engine.cpp`; Qt requires
    // it to run once, on the main thread, before the QGuiApplication is constructed.
    unsafe { od_init_web_engine() };
    let mut app = QGuiApplication::new();
    // The native macOS/Windows styles cannot be restyled; the basic style takes our theme.
    QQuickStyle::set_style(&QString::from("Basic"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                eprintln!("could not load {url}");
                std::process::exit(3);
            })
            .release();
        frame_bridge::install(engine.as_mut());
        engine.as_mut().load(&QUrl::from(MAIN_QML));
    }
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}
