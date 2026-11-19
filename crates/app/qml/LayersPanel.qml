import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Pages and layers, topmost first. Clicking selects; the eye toggles visibility; the arrows move
// the selected layer in the stacking order.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var rows: editor.layers ? JSON.parse(editor.layers) : []
    JsonModel { id: rowsModel }
    onRowsChanged: rowsModel.sync(rows)
    Component.onCompleted: rowsModel.sync(rows)
    readonly property var selectedIds: editor.selection ? JSON.parse(editor.selection) : []

    function isSelected(id) { return selectedIds.indexOf(id) >= 0 }
    function glyph(kind) {
        switch (kind) {
        case "page": return "▤"
        case "frame": return "▢"
        case "group": return "▣"
        case "rect": return "■"
        case "ellipse": return "●"
        case "text": return "T"
        case "image": return "▨"
        default: return "·"
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.pad
            Label {
                text: "Layers"
                color: Theme.text
                font.pixelSize: Theme.fontSize
                font.bold: true
                Layout.fillWidth: true
            }
            Btn {
                text: "▲"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                enabled: root.selectedIds.length > 0
                onClicked: editor.reorder(root.selectedIds[0], 1)
                ToolTip.visible: hovered
                ToolTip.text: "Bring forward"
            }
            Btn {
                text: "▼"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                enabled: root.selectedIds.length > 0
                onClicked: editor.reorder(root.selectedIds[0], -1)
                ToolTip.visible: hovered
                ToolTip.text: "Send backward"
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: rowsModel
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}

            delegate: Rectangle {
                id: row
                required property string json
                readonly property var modelData: JSON.parse(json)
                width: list.width
                height: 26
                color: root.isSelected(modelData.id) ? Theme.panelRaised
                     : hover.hovered ? Qt.rgba(1, 1, 1, 0.04) : "transparent"
                Rectangle {
                    visible: root.isSelected(row.modelData.id)
                    width: 2
                    height: parent.height
                    color: Theme.accent
                }
                HoverHandler { id: hover }
                TapHandler {
                    onTapped: {
                        if (row.modelData.kind === "page") return
                        if (row.modelData.kind === "frame") editor.openFrame(row.modelData.id)
                        editor.select(row.modelData.id)
                    }
                }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8 + row.modelData.depth * 14
                    anchors.rightMargin: 6
                    spacing: 6
                    Label {
                        text: root.glyph(row.modelData.kind)
                        color: row.modelData.kind === "page" ? Theme.accent : Theme.muted
                        font.pixelSize: 11
                        Layout.preferredWidth: 14
                        horizontalAlignment: Text.AlignHCenter
                    }
                    Label {
                        text: row.modelData.name
                        color: row.modelData.visible ? Theme.text : Theme.muted
                        font.pixelSize: Theme.fontSize
                        font.bold: row.modelData.kind === "page"
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: row.modelData.kind !== "page"
                        text: row.modelData.visible ? "👁" : "—"
                        color: row.modelData.visible ? Theme.muted : Theme.danger
                        font.pixelSize: 12
                        opacity: hover.hovered || !row.modelData.visible ? 1 : 0.3
                        TapHandler { onTapped: editor.toggleVisible(row.modelData.id) }
                    }
                }
            }
        }
    }
}
