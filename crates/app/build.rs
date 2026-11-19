use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The Qt modules the editor links. This list is the licensing allowlist from AGENTS.md invariant
/// 12: every entry is offered under the LGPLv3. Adding one needs a licensing check in the PR.
const QT_MODULES: &[&str] = &["Network"];

fn main() {
    // The logo has one source of truth: assets/logo.svg. It is copied next to the QML so it can be
    // part of the QML module's resources.
    println!("cargo:rerun-if-changed=../../assets/logo.svg");
    if let Err(e) = std::fs::copy("../../assets/logo.svg", "qml/logo.svg") {
        println!("cargo:warning=could not copy the logo: {e}");
    }
    let module = QmlModule::new("org.opendesigner.app")
        .qml_file("qml/main.qml")
        .qml_file(QmlFile::from("qml/Theme.qml").singleton(true))
        .qml_files([
            "qml/Btn.qml",
            "qml/Field.qml",
            "qml/JsonModel.qml",
            "qml/VideoPanel.qml",
            "qml/FormatPicker.qml",
            "qml/StartPanel.qml",
            "qml/LayersPanel.qml",
            "qml/InspectorPanel.qml",
            "qml/NotesPanel.qml",
            "qml/HistoryPanel.qml",
            "qml/MemoryPanel.qml",
            "qml/ConnectPanel.qml",
            "qml/Dock.qml",
            "qml/DockNode.qml",
            "qml/DockSplit.qml",
            "qml/DockGroup.qml",
            "qml/ChatPanel.qml",
            "qml/CanvasView.qml",
            "qml/SelfTest.qml",
        ]);
    let mut builder = CxxQtBuilder::new_qml_module(module);
    for m in QT_MODULES {
        builder = builder.qt_module(m);
    }
    builder
        .qrc_resources(["qml/logo.svg"])
        .files(["src/editor.rs"])
        .build();
}
