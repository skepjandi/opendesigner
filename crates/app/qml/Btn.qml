import QtQuick
import QtQuick.Controls

// A flat button in the editor's style. `primary` fills it with the accent; `active` marks a
// tool or toggle that is on.
Button {
    id: root
    property bool primary: false
    property bool active: false
    property color tint: primary ? Theme.textOnAccent : Theme.text

    implicitHeight: Theme.row
    leftPadding: 10
    rightPadding: 10
    hoverEnabled: true
    font.pixelSize: Theme.fontSize

    contentItem: Text {
        text: root.text
        font: root.font
        color: root.enabled ? root.tint : Theme.muted
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: Theme.radius
        color: root.primary ? (root.down ? Theme.accentStrong : Theme.accent)
             : root.active ? Theme.panelRaised
             : root.hovered ? Theme.panelRaised : "transparent"
        border.width: root.active && !root.primary ? 1 : 0
        border.color: Theme.accent
        opacity: root.enabled ? 1 : 0.5
    }
}
