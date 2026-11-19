import QtQuick
import QtQuick.Controls

// A flat button in the editor's style. `primary` fills it with the accent; `active` marks a
// tool or toggle that is on. `iconName` names a vendored vector icon (icons/); with no `text` the
// button is icon-only and `tip` is its tooltip and accessible name.
Button {
    id: root
    property bool primary: false
    property bool active: false
    property color tint: primary ? Theme.textOnAccent : Theme.text
    property string iconName: ""
    property string tip: ""
    readonly property color shade: root.enabled ? root.tint : Theme.muted

    implicitHeight: Theme.row
    leftPadding: 10
    rightPadding: 10
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    Accessible.name: text !== "" ? text : tip
    ToolTip.visible: hovered && tip !== ""
    ToolTip.delay: 600
    ToolTip.text: tip

    contentItem: Item {
        implicitWidth: content.implicitWidth
        implicitHeight: content.implicitHeight
        Row {
            id: content
            anchors.centerIn: parent
            spacing: 6
            Icon {
                visible: root.iconName !== ""
                name: root.iconName
                size: 16
                color: root.shade
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                visible: root.text !== ""
                text: root.text
                font: root.font
                color: root.shade
                verticalAlignment: Text.AlignVCenter
                anchors.verticalCenter: parent.verticalCenter
            }
        }
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
