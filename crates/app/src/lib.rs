//! OpenDesigner editor: a Qt Quick window over the Rust core. The binary is a thin `main` over
//! [`run`]; the code lives in this library so the tests link the generated Qt glue.

pub mod editor;
pub mod state;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

const MAIN_QML: &str = "qrc:/qt/qml/org/opendesigner/app/qml/main.qml";

/// Opens the editor window and runs the event loop. Returns the process exit code.
pub fn run() -> i32 {
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
        engine.as_mut().load(&QUrl::from(MAIN_QML));
    }
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}
