import QtQuick

// A vector icon from the vendored set (icons/<name>.svg), tinted with `color`. Decorative: the
// control that holds it carries the accessible name.
Item {
    id: root
    property string name
    property int size: 16
    property color color: Theme.text

    implicitWidth: size
    implicitHeight: size

    Image {
        anchors.fill: parent
        source: root.name ? Icons.source(root.name, root.color) : ""
        sourceSize.width: root.size * Screen.devicePixelRatio
        sourceSize.height: root.size * Screen.devicePixelRatio
        opacity: root.color.a
        smooth: true
    }
}
