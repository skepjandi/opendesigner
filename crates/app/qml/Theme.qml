pragma Singleton
import QtQuick

// Colors come from the logo: a warm gold ramp over neutral greys (docs/wiki/qt-desktop.md).
QtObject {
    readonly property color canvasBg: "#161514"
    readonly property color panel: "#211f1d"
    readonly property color panelRaised: "#2a2825"
    readonly property color field: "#171615"
    readonly property color border: "#3a3733"
    readonly property color text: "#f2ece0"
    readonly property color muted: "#a09a8e"
    readonly property color accent: "#f2ca83"
    readonly property color accentStrong: "#c7a060"
    readonly property color textOnAccent: "#2a2012"
    // the outline of what an agent changed must not look like the user's selection
    readonly property color agent: "#5ab0ff"
    readonly property color pin: "#ff9f43"
    readonly property color danger: "#ff6b6b"
    readonly property color success: "#7ed39a"
    readonly property int radius: 6
    readonly property int pad: 10
    readonly property int row: 28
    readonly property real fontSize: 13
}
