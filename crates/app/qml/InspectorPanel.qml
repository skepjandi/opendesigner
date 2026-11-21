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
    readonly property var selIds: editor.selection ? JSON.parse(editor.selection) : []
    readonly property bool multi: selIds.length > 1
    readonly property var bounds: editor.selectionBounds && editor.selectionBounds !== "null" ? JSON.parse(editor.selectionBounds) : null
    // keep the width and height in proportion while one is typed (this session only)
    property bool lockAspect: false
    property real stackGap: 16
    function setSize(prop, text) {
        const v = parseFloat(text)
        if (isNaN(v) || !node) return
        if (!lockAspect || node.width <= 0 || node.height <= 0) { setNum(prop, text); return }
        const w = prop === "width" ? v : v * node.width / node.height
        const h = prop === "height" ? v : v * node.height / node.width
        editor.resizeNode(node.id, node.x, node.y, w, h)
    }
    function moveUnion(axis, text) {
        const v = parseFloat(text)
        if (isNaN(v) || !bounds) return
        if (axis === "x") editor.moveSelected(v - bounds.x, 0)
        else editor.moveSelected(0, v - bounds.y)
    }
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

    readonly property var d3: node && node.device3d ? node.device3d : null
    readonly property var models: JSON.parse(editor.deviceModels())
    readonly property var recordings: (editor.memory ? JSON.parse(editor.memory) : []).filter(m => m.kind === "video")
    function d3num(name, fallback) { return d3 && d3[name] !== undefined ? d3[name] : fallback }

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
        visible: !!root.node && !root.multi
        anchors.fill: parent
        contentWidth: availableWidth
        contentHeight: inspectorColumn.implicitHeight + 2 * Theme.pad + 80
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        ColumnLayout {
            id: inspectorColumn
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
                    onCommitted: v => root.setSize("width", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.height) : ""
                    onCommitted: v => root.setSize("height", v)
                }
                Btn {
                    iconName: root.lockAspect ? "lock" : "lock-open"
                    implicitWidth: 28
                    leftPadding: 0
                    rightPadding: 0
                    active: root.lockAspect
                    Accessible.checkable: true
                    Accessible.checked: root.lockAspect
                    tip: root.lockAspect ? "Width and height keep their proportion (click to unlock)" : "Lock the proportion of width and height"
                    onClicked: root.lockAspect = !root.lockAspect
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
                visible: root.node && root.has(root.node.kind, ["frame", "rect", "ellipse", "text", "device3d"])
                label: root.node && root.node.kind === "text" ? "Color" : (root.node && root.node.kind === "device3d" ? "Body" : "Fill")
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
            Row2 {
                visible: root.node && root.node.kind !== "frame"
                label: "Rotate"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.rotation || 0) : ""
                    onCommitted: v => root.setNum("rotation", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind !== "frame"
                label: "Scale"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.scale_x === undefined ? 1 : root.node.scale_x) : ""
                    onCommitted: v => root.setNum("scale_x", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.scale_y === undefined ? 1 : root.node.scale_y) : ""
                    onCommitted: v => root.setNum("scale_y", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind !== "frame"
                label: "Pivot"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.pivot_x === undefined ? 0.5 : root.node.pivot_x) : ""
                    onCommitted: v => root.setNum("pivot_x", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.pivot_y === undefined ? 0.5 : root.node.pivot_y) : ""
                    onCommitted: v => root.setNum("pivot_y", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind !== "frame"
                label: "Blur"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.blur || 0) : ""
                    onCommitted: v => root.setNum("blur", v)
                }
            }
            Row2 {
                visible: root.node && root.node.kind !== "frame"
                label: "Depth"
                Field {
                    Layout.fillWidth: true
                    text: root.node ? root.num(root.node.depth === undefined ? 1 : root.node.depth) : ""
                    onCommitted: v => root.setNum("depth", v)
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
                visible: root.node && root.has(root.node.kind, ["image", "device3d"])
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
                visible: root.node && root.has(root.node.kind, ["image", "device3d"])
                label: root.node && root.node.kind === "device3d" ? "Screen" : "File"
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
            ColumnLayout {
                id: deviceSection
                visible: !!root.d3
                Layout.fillWidth: true
                spacing: 8
                Row2 {
                    label: "Model"
                    ComboBox {
                        Layout.fillWidth: true
                        model: root.models.map(m => m.label)
                        currentIndex: Math.max(0, root.models.findIndex(m => m.id === root.d3num("model", "phone")))
                        onActivated: i => root.editor.setNodeProperty(root.node.id, "model", JSON.stringify(root.models[i].id))
                        Accessible.name: "3D model"
                    }
                }
                // a number of the device: a slider that commits when released, and a field
                Repeater {
                    model: [
                        { label: "Tilt", prop: "rotate_x", low: -180, high: 180, fallback: 0, decimals: 1 },
                        { label: "Turn", prop: "rotate_y", low: -180, high: 180, fallback: 0, decimals: 1 },
                        { label: "Roll", prop: "rotate_z", low: -180, high: 180, fallback: 0, decimals: 1 },
                        { label: "Zoom", prop: "zoom", low: 0.05, high: 10, fallback: 1, decimals: 2 },
                        { label: "Lens", prop: "fov", low: 1, high: 100, fallback: 30, decimals: 1 },
                        { label: "Light", prop: "light_angle", low: -360, high: 360, fallback: -35, decimals: 0 },
                        { label: "Shadow", prop: "shadow", low: 0, high: 1, fallback: 0.35, decimals: 2 },
                        { label: "Glare", prop: "glare", low: 0, high: 1, fallback: 0.2, decimals: 2 }
                    ]
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 8
                        Label {
                            text: modelData.label
                            color: Theme.muted
                            font.pixelSize: 12
                            Layout.preferredWidth: 64
                        }
                        Slider {
                            id: slide
                            Layout.fillWidth: true
                            from: modelData.low
                            to: modelData.high
                            // the value the user dragged to; the binding puts it back when the
                            // document answers, so the drag is read from here
                            property real dragged: value
                            onMoved: dragged = value
                            onPressedChanged: {
                                if (pressed || !root.node) return
                                const now = root.d3num(modelData.prop, modelData.fallback)
                                if (Math.abs(dragged - now) > 1e-9)
                                    root.editor.setNodeProperty(root.node.id, modelData.prop, JSON.stringify(dragged))
                            }
                            Binding {
                                target: slide
                                property: "value"
                                value: root.d3num(modelData.prop, modelData.fallback)
                                when: !slide.pressed
                            }
                            Accessible.name: modelData.label
                        }
                        Field {
                            Layout.preferredWidth: 60
                            text: {
                                const now = root.d3num(modelData.prop, modelData.fallback)
                                const p = Math.pow(10, modelData.decimals)
                                return (Math.round(now * p) / p).toString()
                            }
                            onCommitted: v => {
                                const n = parseFloat(v)
                                if (!isNaN(n) && root.node)
                                    root.editor.setNodeProperty(root.node.id, modelData.prop, JSON.stringify(n))
                            }
                            Accessible.name: modelData.label + " value"
                        }
                    }
                }
                Row2 {
                    label: "Scroll"
                    Slider {
                        id: scrollSlide
                        Layout.fillWidth: true
                        from: 0
                        to: 1
                        property real dragged: value
                        onMoved: dragged = value
                        onPressedChanged: {
                            if (pressed || !root.node) return
                            const now = root.d3num("screen_scroll", 0.5)
                            if (Math.abs(dragged - now) > 1e-9)
                                root.editor.setNodeProperty(root.node.id, "screen_scroll", JSON.stringify(dragged))
                        }
                        Binding { target: scrollSlide; property: "value"; value: root.d3num("screen_scroll", 0.5); when: !scrollSlide.pressed }
                        Accessible.name: "Screen scroll"
                    }
                    Btn {
                        text: root.d3 && root.d3.screen_scroll !== undefined ? "Centre" : "Set"
                        tip: root.d3 && root.d3.screen_scroll !== undefined
                             ? "Go back to the centred picture (a capture taller than the screen shows its middle)"
                             : "Scroll a capture taller than the screen: start at its middle"
                        onClicked: root.editor.setNodeProperty(root.node.id, "screen_scroll", root.d3 && root.d3.screen_scroll !== undefined ? "null" : "0.5")
                    }
                }
                Row2 {
                    label: "Recording"
                    ComboBox {
                        id: recordingBox
                        Layout.fillWidth: true
                        readonly property string currentFile: root.d3 && root.d3.screen_footage ? root.d3.screen_footage.file : ""
                        readonly property var files: [""].concat(root.recordings.map(r => r.id))
                        model: files.map(f => f === "" ? "None (show the picture)" : f.replace("media/", ""))
                        currentIndex: Math.max(0, files.indexOf(currentFile))
                        onActivated: i => root.editor.setScreenRecording(root.node.id, files[i])
                        Accessible.name: "Screen recording"
                    }
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.recordings.length === 0
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: "Drop a video (mp4, mov, m4v) on the device to put a recording on its screen, or a picture to show a capture. Alt-drag the device to turn it."
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

    // several layers: what they are together, and how to line them up
    ScrollView {
        visible: root.multi
        anchors.fill: parent
        contentWidth: availableWidth
        contentHeight: multiColumn.implicitHeight + 2 * Theme.pad + 80
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        ColumnLayout {
            id: multiColumn
            width: root.width - 2 * Theme.pad
            x: Theme.pad
            y: Theme.pad
            spacing: 8

            Label {
                text: root.selIds.length + " LAYERS SELECTED"
                color: Theme.accent
                font.pixelSize: 11
                font.bold: true
            }
            Row2 {
                label: "Position"
                Field {
                    Layout.fillWidth: true
                    text: root.bounds ? root.num(root.bounds.x) : ""
                    onCommitted: v => root.moveUnion("x", v)
                }
                Field {
                    Layout.fillWidth: true
                    text: root.bounds ? root.num(root.bounds.y) : ""
                    onCommitted: v => root.moveUnion("y", v)
                }
            }
            Row2 {
                label: "Size"
                Label { Layout.fillWidth: true; color: Theme.muted; font.pixelSize: 12; text: root.bounds ? root.num(root.bounds.width) : "" }
                Label { Layout.fillWidth: true; color: Theme.muted; font.pixelSize: 12; text: root.bounds ? root.num(root.bounds.height) : "" }
            }
            Row2 {
                label: "Align"
                ArrangeGroup { editor: root.editor; group: "align"; count: root.selIds.length }
            }
            Row2 {
                label: "Distribute"
                ArrangeGroup { editor: root.editor; group: "distribute"; count: root.selIds.length }
            }
            Row2 {
                label: "Stack"
                ArrangeGroup { editor: root.editor; group: "stack"; count: root.selIds.length; gap: root.stackGap }
                Field {
                    Layout.preferredWidth: 48
                    placeholderText: "Gap"
                    text: root.num(root.stackGap)
                    onCommitted: v => { const g = parseFloat(v); if (!isNaN(g)) root.stackGap = g }
                }
            }
            Row2 {
                label: "Layers"
                Btn { text: "Group"; enabled: root.editor.canGroup === true; onClicked: root.editor.groupSelected() }
                Btn { text: "Duplicate"; onClicked: root.editor.duplicateSelected() }
            }
        }
    }
}
