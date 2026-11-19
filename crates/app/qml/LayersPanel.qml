import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Pages and layers, topmost first. Clicking selects; the eye toggles visibility; the arrows move
// the selected layer in the stacking order. A layer with children folds and unfolds; the folded
// ones are remembered by id for the session, and selecting a layer unfolds the ones above it.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var rows: editor.layers ? JSON.parse(editor.layers) : []
    readonly property var selectedIds: editor.selection ? JSON.parse(editor.selection) : []
    property var collapsed: ({})
    // the rows that show: those under a folded layer are left out
    readonly property var shownRows: {
        const out = []
        let hideBelow = -1
        for (const r of rows) {
            if (hideBelow >= 0 && r.depth > hideBelow) continue
            hideBelow = -1
            const folded = r.has_children && collapsed[r.id] === true
            out.push(Object.assign({}, r, { folded: folded }))
            if (folded) hideBelow = r.depth
        }
        return out
    }
    readonly property int shownCount: rowsModel.count
    JsonModel { id: rowsModel }
    onShownRowsChanged: rowsModel.sync(shownRows)
    Component.onCompleted: rowsModel.sync(shownRows)
    onSelectedIdsChanged: Qt.callLater(revealSelection)

    function isSelected(id) { return selectedIds.indexOf(id) >= 0 }
    function icon(kind) {
        switch (kind) {
        case "page": return "file"
        case "frame": return "frame"
        case "group": return "group"
        case "rect": return "square"
        case "ellipse": return "circle"
        case "text": return "type"
        case "image": return "image"
        default: return "square"
        }
    }
    function toggleFold(id) {
        const next = Object.assign({}, collapsed)
        if (next[id]) delete next[id]
        else next[id] = true
        collapsed = next
    }
    function expandAll() { collapsed = ({}) }
    function collapseAll() {
        const next = {}
        for (const r of rows) if (r.has_children && r.kind !== "page") next[r.id] = true
        collapsed = next
    }
    // unfold every layer above the selected one so it can be seen in the list
    function revealSelection() {
        const i = rows.findIndex(r => r.id === selectedIds[0])
        if (i < 0) return
        let depth = rows[i].depth
        const next = Object.assign({}, collapsed)
        let changed = false
        for (let j = i - 1; j >= 0 && depth > 0; --j) {
            if (rows[j].depth >= depth) continue
            depth = rows[j].depth
            if (next[rows[j].id]) { delete next[rows[j].id]; changed = true }
        }
        if (changed) collapsed = next
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.pad
            spacing: 2
            Label {
                text: "Layers"
                color: Theme.text
                font.pixelSize: Theme.fontSize
                font.bold: true
                Layout.fillWidth: true
            }
            Btn {
                iconName: "chevrons-up-down"
                tip: "Expand all"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                onClicked: root.expandAll()
            }
            Btn {
                iconName: "chevrons-down-up"
                tip: "Collapse all"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                onClicked: root.collapseAll()
            }
            Btn {
                iconName: "arrow-up"
                tip: "Bring forward"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                enabled: root.selectedIds.length > 0
                onClicked: editor.reorder(root.selectedIds[0], 1)
            }
            Btn {
                iconName: "arrow-down"
                tip: "Send backward"
                implicitWidth: 28
                leftPadding: 0
                rightPadding: 0
                enabled: root.selectedIds.length > 0
                onClicked: editor.reorder(root.selectedIds[0], -1)
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
                    anchors.leftMargin: 6 + row.modelData.depth * 14
                    anchors.rightMargin: 6
                    spacing: 6
                    Item {
                        Layout.preferredWidth: 14
                        Layout.preferredHeight: 14
                        Icon {
                            anchors.centerIn: parent
                            visible: row.modelData.has_children
                            name: row.modelData.folded ? "chevron-right" : "chevron-down"
                            size: 14
                            color: Theme.muted
                        }
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -4
                            enabled: row.modelData.has_children
                            onClicked: root.toggleFold(row.modelData.id)
                            Accessible.role: Accessible.Button
                            Accessible.name: (row.modelData.folded ? "Expand " : "Collapse ") + row.modelData.name
                        }
                    }
                    Icon {
                        name: root.icon(row.modelData.kind)
                        size: 14
                        color: row.modelData.kind === "page" ? Theme.accent : Theme.muted
                    }
                    Label {
                        text: row.modelData.name
                        color: row.modelData.visible ? Theme.text : Theme.muted
                        font.pixelSize: Theme.fontSize
                        font.bold: row.modelData.kind === "page"
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Item {
                        visible: row.modelData.kind !== "page"
                        Layout.preferredWidth: 16
                        Layout.preferredHeight: 16
                        opacity: hover.hovered || !row.modelData.visible ? 1 : 0.3
                        Icon {
                            anchors.centerIn: parent
                            name: row.modelData.visible ? "eye" : "eye-off"
                            size: 14
                            color: row.modelData.visible ? Theme.muted : Theme.danger
                        }
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -4
                            onClicked: editor.toggleVisible(row.modelData.id)
                            Accessible.role: Accessible.Button
                            Accessible.name: row.modelData.visible ? "Hide " + row.modelData.name : "Show " + row.modelData.name
                        }
                    }
                }
            }
        }
    }
}
