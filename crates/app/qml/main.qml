import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.opendesigner.app

ApplicationWindow {
    id: win
    visible: true
    width: 1440
    height: 900
    minimumWidth: 980
    minimumHeight: 620
    color: Theme.canvasBg
    title: backend.hasProject ? "OpenDesigner — " + backend.projectName : "OpenDesigner"

    Editor { id: backend }

    property string tool: "select"
    readonly property bool typing: activeFocusItem !== null && activeFocusItem.hasOwnProperty("cursorPosition")
    readonly property var frames: backend.frames ? JSON.parse(backend.frames) : []

    function toast(text, isError) {
        toastLabel.text = text
        toastBox.error = !!isError
        toastBox.opacity = 1
        toastTimer.restart()
    }
    Connections {
        target: backend
        function onMessageSeqChanged() { win.toast(backend.message, backend.messageIsError) }
    }

    // ---------- shortcuts ----------
    Shortcut { sequences: [StandardKey.Undo]; onActivated: backend.undo() }
    Shortcut { sequences: [StandardKey.Redo]; onActivated: backend.redo() }
    Shortcut { sequences: [StandardKey.Save]; onActivated: backend.saveProject() }
    Shortcut { sequences: ["Delete", "Backspace"]; enabled: !win.typing && backend.hasProject; onActivated: backend.deleteSelected() }
    Shortcut { sequences: ["Escape"]; onActivated: { win.tool = "select"; backend.select("") } }
    Shortcut { sequence: "V"; enabled: !win.typing; onActivated: win.tool = "select" }
    Shortcut { sequence: "R"; enabled: !win.typing; onActivated: win.tool = "rect" }
    Shortcut { sequence: "O"; enabled: !win.typing; onActivated: win.tool = "ellipse" }
    Shortcut { sequence: "T"; enabled: !win.typing; onActivated: win.tool = "text" }
    Shortcut { sequence: "F"; enabled: !win.typing; onActivated: win.tool = "frame" }
    Shortcut { sequence: "P"; enabled: !win.typing; onActivated: win.tool = "pin" }
    Shortcut { sequences: [StandardKey.ZoomIn]; onActivated: canvas.zoomCenter(1.25) }
    Shortcut { sequences: [StandardKey.ZoomOut]; onActivated: canvas.zoomCenter(0.8) }
    Shortcut { sequence: "Ctrl+0"; onActivated: canvas.fit() }

    // ---------- layout ----------
    ColumnLayout {
        id: shell
        anchors.fill: parent
        spacing: 0
        visible: backend.hasProject

        // toolbar
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 46
            color: Theme.panel
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.pad
                anchors.rightMargin: Theme.pad
                spacing: 4

                Image {
                    source: "qrc:/qt/qml/org/opendesigner/app/qml/logo.svg"
                    sourceSize.height: 28
                    Layout.preferredHeight: 28
                    fillMode: Image.PreserveAspectFit
                }
                Label {
                    text: backend.projectName
                    color: Theme.text
                    font.pixelSize: 14
                    font.bold: true
                    Layout.rightMargin: 12
                }
                Repeater {
                    model: [
                        { id: "select", label: "Select", key: "V" },
                        { id: "rect", label: "Rectangle", key: "R" },
                        { id: "ellipse", label: "Ellipse", key: "O" },
                        { id: "text", label: "Text", key: "T" },
                        { id: "frame", label: "Frame", key: "F" },
                        { id: "pin", label: "Pin a note", key: "P" }
                    ]
                    delegate: Btn {
                        required property var modelData
                        text: modelData.label
                        active: win.tool === modelData.id
                        onClicked: win.tool = modelData.id
                        ToolTip.visible: hovered
                        ToolTip.delay: 600
                        ToolTip.text: modelData.label + " (" + modelData.key + ")"
                    }
                }
                Btn {
                    text: "Image…"
                    enabled: backend.activeFrame !== ""
                    onClicked: imageDialog.open()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: "Add a PNG, JPEG or WebP image (or drop one on the canvas)"
                }
                Item { Layout.fillWidth: true }
                Btn { text: "Undo"; enabled: backend.canUndo; onClicked: backend.undo() }
                Btn { text: "Redo"; enabled: backend.canRedo; onClicked: backend.redo() }
                Rectangle { width: 1; height: 22; color: Theme.border; Layout.leftMargin: 6; Layout.rightMargin: 6 }
                Btn { text: "−"; implicitWidth: 30; leftPadding: 0; rightPadding: 0; onClicked: canvas.zoomCenter(0.8) }
                Label { text: Math.round(canvas.zoom * 100) + "%"; color: Theme.muted; Layout.preferredWidth: 44; horizontalAlignment: Text.AlignHCenter }
                Btn { text: "+"; implicitWidth: 30; leftPadding: 0; rightPadding: 0; onClicked: canvas.zoomCenter(1.25) }
                Btn { text: "Fit"; onClicked: canvas.fit() }
                Rectangle { width: 1; height: 22; color: Theme.border; Layout.leftMargin: 6; Layout.rightMargin: 6 }
                Btn { text: "Open…"; onClicked: openDialog.open() }
                Btn { text: "Save"; onClicked: backend.saveProject() }
                Btn { text: "Export…"; primary: true; onClicked: exportDialog.openDialog() }
            }
        }

        SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal
            handle: Rectangle { implicitWidth: 1; color: Theme.border }

            LayersPanel {
                editor: backend
                SplitView.preferredWidth: 250
                SplitView.minimumWidth: 180
                SplitView.maximumWidth: 420
            }

            ColumnLayout {
                SplitView.fillWidth: true
                spacing: 0
                // frame switcher
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 34
                    color: Theme.panel
                    visible: win.frames.length > 0
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
                    ListView {
                        anchors.fill: parent
                        anchors.leftMargin: 6
                        orientation: ListView.Horizontal
                        model: win.frames
                        spacing: 2
                        clip: true
                        delegate: Btn {
                            required property var modelData
                            text: modelData.name + "  " + Math.round(modelData.width) + "×" + Math.round(modelData.height)
                            active: modelData.id === backend.activeFrame
                            height: 34
                            onClicked: { backend.openFrame(modelData.id); backend.select(modelData.id) }
                        }
                    }
                }
                CanvasView {
                    id: canvas
                    editor: backend
                    tool: win.tool
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    onToolFinished: win.tool = "select"
                    onRequestPin: (nodeId, x, y, w, h) => pinDialog.openFor(nodeId, x, y, w, h)
                    onNoteClicked: id => { agent.tab = 0 }
                }
            }

            SplitView {
                orientation: Qt.Vertical
                SplitView.preferredWidth: 320
                SplitView.minimumWidth: 260
                SplitView.maximumWidth: 520
                handle: Rectangle { implicitHeight: 1; color: Theme.border }
                InspectorPanel {
                    editor: backend
                    SplitView.preferredHeight: 360
                    SplitView.minimumHeight: 160
                    onRequestPin: nodeId => pinDialog.openFor(nodeId, 0, 0, 0, 0)
                }
                AgentPanel {
                    id: agent
                    editor: backend
                    onToast: text => win.toast(text, false)
                    SplitView.fillHeight: true
                    SplitView.minimumHeight: 200
                }
            }
        }
    }

    // ---------- welcome ----------
    Rectangle {
        anchors.fill: parent
        visible: !backend.hasProject
        color: Theme.canvasBg
        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(520, parent.width - 60)
            spacing: 14
            Image {
                source: "qrc:/qt/qml/org/opendesigner/app/qml/logo.svg"
                sourceSize.height: 140
                Layout.preferredHeight: 140
                Layout.alignment: Qt.AlignHCenter
                fillMode: Image.PreserveAspectFit
            }
            Label {
                text: "OpenDesigner"
                color: Theme.text
                font.pixelSize: 30
                font.bold: true
                Layout.alignment: Qt.AlignHCenter
            }
            Label {
                text: "Design store and web assets with an agent, guiding it by pointing."
                color: Theme.muted
                font.pixelSize: 14
                Layout.alignment: Qt.AlignHCenter
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: 10
                Btn { text: "New project…"; primary: true; implicitHeight: 36; onClicked: newDialog.openDialog() }
                Btn { text: "Open project…"; implicitHeight: 36; onClicked: openDialog.open() }
            }
        }
    }

    // ---------- dialogs ----------
    component Modal: Dialog {
        id: dlg
        modal: true
        anchors.centerIn: Overlay.overlay
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: dlg.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }
    }

    FolderDialog { id: openDialog; title: "Open an OpenDesigner project folder"; onAccepted: backend.openProject(selectedFolder.toString()) }
    FileDialog {
        id: imageDialog
        title: "Add an image"
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp)"]
        onAccepted: backend.importImage(selectedFile.toString())
    }
    FolderDialog { id: parentDialog; title: "Where should the project folder go?"; onAccepted: newDialog.parentUrl = selectedFolder.toString() }

    Modal {
        id: newDialog
        title: "New project"
        width: 520
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string parentUrl: ""
        property var presets: []
        function openDialog() {
            presets = JSON.parse(backend.framePresets())
            nameField.text = "My store assets"
            sizeBox.currentIndex = 0
            open()
        }
        contentItem: ColumnLayout {
            spacing: 10
            Label { text: "Name"; color: Theme.muted; font.pixelSize: 12 }
            Field { id: nameField; Layout.fillWidth: true }
            Label { text: "First frame"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                id: sizeBox
                Layout.fillWidth: true
                model: newDialog.presets.map(p => p.label)
            }
            Label { text: "Folder"; color: Theme.muted; font.pixelSize: 12 }
            RowLayout {
                Label {
                    Layout.fillWidth: true
                    text: newDialog.parentUrl === "" ? "Choose where to create it" : backend.localPath(newDialog.parentUrl)
                    color: newDialog.parentUrl === "" ? Theme.muted : Theme.text
                    elide: Text.ElideMiddle
                }
                Btn { text: "Choose…"; onClicked: parentDialog.open() }
            }
        }
        onAccepted: {
            if (parentUrl === "" || nameField.text.trim() === "") { win.toast("Choose a folder and a name", true); return }
            const p = presets[sizeBox.currentIndex]
            const dir = backend.localPath(parentUrl) + "/" + nameField.text.trim().replace(/[\/\\]/g, "-") + ".od"
            backend.newProject(dir, nameField.text.trim(), p.width, p.height)
        }
    }

    Modal {
        id: pinDialog
        title: "Pin a note"
        width: 460
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string nodeId: ""
        property real rx: 0
        property real ry: 0
        property real rw: 0
        property real rh: 0
        function openFor(id, x, y, w, h) {
            nodeId = id; rx = x; ry = y; rw = w; rh = h
            noteText.text = ""
            open()
            noteText.forceActiveFocus()
        }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 12
                text: pinDialog.nodeId !== "" ? "Tell the agent what to change about this layer." : "Tell the agent what to change in this area."
            }
            TextArea {
                id: noteText
                Layout.fillWidth: true
                Layout.preferredHeight: 100
                wrapMode: TextEdit.Wrap
                color: Theme.text
                font.pixelSize: Theme.fontSize
                placeholderText: "e.g. headline too small, more contrast"
                background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: noteText.activeFocus ? Theme.pin : Theme.border }
            }
        }
        onAccepted: {
            if (noteText.text.trim() === "") return
            if (nodeId !== "") backend.addPin(noteText.text.trim(), nodeId)
            else backend.addRegionPin(noteText.text.trim(), rx, ry, rw, rh)
            agent.tab = 0
        }
    }

    Modal {
        id: exportDialog
        title: "Export the open frame"
        width: 540
        standardButtons: Dialog.Ok | Dialog.Cancel
        property var presets: []
        function openDialog() {
            presets = JSON.parse(backend.framePresets()).filter(p => p.id.indexOf("web-") !== 0)
            fileField.text = "shot-01.png"
            presetBox.currentIndex = 0
            open()
        }
        contentItem: ColumnLayout {
            spacing: 8
            Label { text: "Check against"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                id: presetBox
                Layout.fillWidth: true
                model: ["No store preset"].concat(exportDialog.presets.map(p => p.label))
            }
            Label { text: "File name (saved in the project's exports folder)"; color: Theme.muted; font.pixelSize: 12 }
            Field { id: fileField; Layout.fillWidth: true }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "A file that does not meet the chosen store's rules is not written; you are told which rule it breaks."
            }
        }
        onAccepted: {
            const preset = presetBox.currentIndex > 0 ? exportDialog.presets[presetBox.currentIndex - 1].id : ""
            backend.exportFrame(preset, fileField.text.trim())
        }
    }

    // ---------- toast ----------
    Rectangle {
        id: toastBox
        property bool error: false
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        width: Math.min(parent.width - 40, toastLabel.implicitWidth + 32)
        height: toastLabel.implicitHeight + 20
        radius: Theme.radius
        color: error ? "#4a1f1f" : Theme.panelRaised
        border.width: 1
        border.color: error ? Theme.danger : Theme.border
        opacity: 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 160 } }
        Label {
            id: toastLabel
            anchors.centerIn: parent
            width: parent.width - 32
            wrapMode: Text.WordWrap
            color: toastBox.error ? "#ffd6d6" : Theme.text
            font.pixelSize: Theme.fontSize
        }
        Timer { id: toastTimer; interval: 4200; onTriggered: toastBox.opacity = 0 }
    }

    SelfTest { editor: backend; canvas: canvas; grabTarget: shell }
}
