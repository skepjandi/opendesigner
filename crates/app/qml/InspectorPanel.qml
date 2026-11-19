import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Properties of the selected node. Every edit is one operation as the user; the core validates it
// and a rejection comes back as a message.
Rectangle {
    id: root
    required property var editor
    signal requestPin(string nodeId)
    signal requestReplaceImage(string nodeId)
    color: Theme.panel

    readonly property var node: editor.inspector && editor.inspector !== "null" ? JSON.parse(editor.inspector) : null
    readonly property var overflow: {
        const all = editor.overflows ? JSON.parse(editor.overflows) : []
        return node ? all.find(o => o.id === node.id) || null : null
    }

    // focus the text of the selected text layer (Return)
    function editText() {
        if (node && node.kind === "text") { textEdit.forceActiveFocus(); textEdit.selectAll() }
    }
    function num(v) { return (Math.round(v * 100) / 100).toString() }
    function setNum(prop, text) {
        const v = parseFloat(text)
        if (!isNaN(v) && node) editor.setNodeProperty(node.id, prop, JSON.stringify(v))
    }
    function has(kind, kinds) { return kinds.indexOf(kind) >= 0 }

    component Row2: RowLayout {
        property alias label: lab.text
        default property alias content: slot.data
        Layout.fillWidth: true
        spacing: 8
        Label {
            id: lab
            color: Theme.muted
            font.pixelSize: 12
            Layout.preferredWidth: 64
        }
        RowLayout { id: slot; Layout.fillWidth: true; spacing: 6 }
    }

    Label {
        visible: !root.node
        anchors.centerIn: parent
        width: parent.width - 32
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: Theme.fontSize
        text: "Select a layer on the canvas or in the list to edit it. Pin a note on it to guide an agent."
    }

    ScrollView {
        visible: !!root.node
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: root.width - 2 * Theme.pad
            x: Theme.pad
            y: Theme.pad
            spacing: 8

            Label {
                text: root.node ? root.node.kind.toUpperCase() : ""
                color: Theme.accent
                font.pixelSize: 11
                font.bold: true
            }
            Row2 {
                label: "Name"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.node.name : ""
                    onCommitted: v => { if (root.node && v !== root.node.name) editor.setNodeProperty(root.node.id, "name", JSON.stringify(v)) }
                }
            }
            Row2 {
                label: "Position"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.x) : ""
                    onCommitted: v => root.setNum("x", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.y) : ""
                    onCommitted: v => root.setNum("y", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind !== "group"
                label: "Size"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.width) : ""
                    onCommitted: v => root.setNum("width", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.height) : ""
                    onCommitted: v => root.setNum("height", v)
                }
            }
            Row2 {
                label: "Opacity"
                Slider {
                    id: opacity
                    Layout.fillWidth: true
                    from: 0
                    to: 1
                    value: root.node && root.node.opacity !== undefined ? root.node.opacity : 1
                    onMoved: root.setNum("opacity", value.toFixed(2))
                }
                Label {
                    text: Math.round(opacity.value * 100) + "%"
                    color: Theme.muted
                    font.pixelSize: 12
                    Layout.preferredWidth: 36
                }
            }
            Row2 {
                visible: root.node && root.has(root.node.kind, ["frame", "rect", "ellipse", "text"])
                label: root.node && root.node.kind === "text" ? "Color" : "Fill"
                Rectangle {
                    width: 22
                    height: 22
                    radius: 4
                    border.width: 1
                    border.color: Theme.border
                    color: root.node && root.node.fill && root.node.fill.charAt(0) === "#" ? root.node.fill : "transparent"
                }
                Field {
                    Layout.fillWidth: true
                    placeholderText: "#rrggbb or {token}"
                    text: root.node && root.node.fill ? root.node.fill : ""
                    onCommitted: v => {
                        if (!root.node || v === (root.node.fill || "")) return
                        editor.setNodeProperty(root.node.id, "fill", v === "" ? "null" : JSON.stringify(v))
                    }
                }
            }
            Row2 {
                visible: root.node && root.has(root.node.kind, ["frame", "rect", "image"])
                label: "Radius"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.corner_radius || 0) : ""
                    onCommitted: v => root.setNum("corner_radius", v)
                }
            }
            Label {
                Layout.fillWidth: true
                visible: root.overflow !== null
                wrapMode: Text.WordWrap
                color: Theme.danger
                font.pixelSize: 11
                text: root.overflow ? "The text needs " + root.overflow.needed + " px of height but the box is " + root.overflow.available + " px. Make the box taller, or the text smaller or shorter." : ""
            }
            Row2 {
                visible: root.node && root.node.kind === "text"
                label: "Size"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.font_size || 16) : ""
                    onCommitted: v => root.setNum("font_size", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind === "text"
                label: "Font"
                Field {
                    Layout.fillWidth: true
                    placeholderText: "Lato (bundled) or an installed family"
                    text: root.node && root.node.font_family ? root.node.font_family : ""
                    onCommitted: v => {
                        if (!root.node || v === (root.node.font_family || "")) return
                        editor.setNodeProperty(root.node.id, "font_family", v === "" ? "null" : JSON.stringify(v))
                    }
                }
            }
            Row2 {
                visible: root.node && root.node.kind === "text"
                label: "Weight"
                Field {
                    Layout.fillWidth: true
                    placeholderText: "100 to 900 (400 regular, 700 bold)"
                    text: root.node && root.node.font_weight ? root.num(root.node.font_weight) : ""
                    onCommitted: v => {
                        if (!root.node) return
                        if (v === "") editor.setNodeProperty(root.node.id, "font_weight", "null")
                        else root.setNum("font_weight", v)
                    }
                }
                Field {
                    Layout.preferredWidth: 60
                    placeholderText: "Line"
                    text: root.node && root.node.line_height ? root.num(root.node.line_height) : ""
                    onCommitted: v => {
                        if (!root.node) return
                        if (v === "") editor.setNodeProperty(root.node.id, "line_height", "null")
                        else root.setNum("line_height", v)
                    }
                }
            }
            Row2 {
                visible: root.node && root.node.kind === "text"
                label: "Align"
                Repeater {
                    model: ["left", "center", "right"]
                    delegate: Btn {
                        required property string modelData
                        text: modelData
                        active: (root.node && root.node.text_align || "left") === modelData
                        onClicked: editor.setNodeProperty(root.node.id, "text_align", JSON.stringify(modelData))
                    }
                }
            }
            Row2 {
                visible: root.node && root.node.kind === "image"
                label: "Fit"
                Repeater {
                    model: ["cover", "contain", "fill"]
                    delegate: Btn {
                        required property string modelData
                        text: modelData
                        active: (root.node && root.node.image_fit || "cover") === modelData
                        onClicked: editor.setNodeProperty(root.node.id, "image_fit", JSON.stringify(modelData))
                    }
                }
            }
            Row2 {
                visible: root.node && root.node.kind === "image"
                label: "File"
                Label {
                    Layout.fillWidth: true
                    text: root.node && root.node.image ? root.node.image : "none"
                    color: Theme.muted
                    font.pixelSize: 12
                    elide: Text.ElideMiddle
                }
                Btn {
                    text: "Replace…"
                    onClicked: root.requestReplaceImage(root.node.id)
                }
            }
            ColumnLayout {
                visible: root.node && root.node.kind === "text"
                Layout.fillWidth: true
                spacing: 4
                Label {
                    text: root.node && root.node.translated !== undefined
                          ? "Text (" + editor.language + (root.node.translated ? "" : ", not translated yet") + ")"
                          : "Text"
                    color: Theme.muted
                    font.pixelSize: 12
                }
                TextArea {
                    id: textEdit
                    Layout.fillWidth: true
                    Layout.preferredHeight: 80
                    wrapMode: TextEdit.Wrap
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    selectionColor: Theme.accentStrong
                    text: root.node && root.node.text ? root.node.text : ""
                    background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: parent.activeFocus ? Theme.accent : Theme.border }
                    onEditingFinished: { if (root.node && text !== root.node.text) editor.setNodeProperty(root.node.id, "text", JSON.stringify(text)) }
                }
            }
            Row2 {
                label: "Pin note"
                Btn {
                    text: "Add a note…"
                    onClicked: root.requestPin(root.node ? root.node.id : "")
                }
            }
            Label {
                visible: root.node && root.node.child_count > 0
                text: root.node ? root.node.child_count + " child layers" : ""
                color: Theme.muted
                font.pixelSize: 12
            }
        }
    }
}
