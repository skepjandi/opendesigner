import QtQuick
import QtQuick.Controls

// A text input in the editor's style. `committed(text)` fires on Enter or when focus leaves.
TextField {
    id: root
    signal committed(string value)

    implicitHeight: Theme.row
    color: Theme.text
    selectionColor: Theme.accentStrong
    selectedTextColor: Theme.textOnAccent
    placeholderTextColor: Theme.muted
    font.pixelSize: Theme.fontSize
    leftPadding: 8
    rightPadding: 8
    selectByMouse: true

    background: Rectangle {
        radius: 4
        color: Theme.field
        border.width: 1
        border.color: root.activeFocus ? Theme.accent : Theme.border
    }
    onEditingFinished: committed(text)
}
