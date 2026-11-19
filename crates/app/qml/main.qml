import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQml
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
    // the panels are placed by the dock (`Dock.qml`, `layout.rs`); the ones other code talks to
    // register themselves here when they are created
    property var canvas: null
    property var panels: ({})
    function zoomBy(f) { if (canvas) canvas.zoomCenter(f) }
    function zoomTo(z) { if (canvas) canvas.zoomTo(z) }
    function fitCanvas() { if (canvas) canvas.fit() }
    function fitSelection() { if (canvas) canvas.fitSelection() }
    // bring a panel into view wherever it is: show it if hidden, make its tab the active one and
    // raise its window if it is in one
    function showPanel(id) {
        const w = backend.layoutReveal(id)
        if (w > 0) for (let i = 0; i < floatingWindows.count; ++i) {
            const fw = floatingWindows.objectAt(i)
            if (fw && fw.windowId === w) { fw.raise(); fw.requestActivate() }
        }
    }
    function focusChat() {
        showPanel("chat")
        focusTimer.restart()
    }
    Timer { id: focusTimer; interval: 80; onTriggered: { const c = win.panels.chat; if (c) c.focusInput() } }
    // what this workspace is for: the format, who chose it, and a change the agent asked for
    readonly property var brief: backend.brief && backend.brief !== "null" ? JSON.parse(backend.brief) : null
    readonly property var briefFormat: brief ? brief.format : null
    readonly property var pendingFormat: brief ? brief.pending_request : null
    readonly property bool typing: activeFocusItem !== null && activeFocusItem.hasOwnProperty("cursorPosition")
    readonly property var frames: backend.frames ? JSON.parse(backend.frames) : []
    JsonModel { id: framesModel }
    onFramesChanged: framesModel.sync(frames)

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
    // Keys follow Figma and Penpot where they agree (see docs/wiki/qt-desktop.md#shortcuts). On
    // macOS Qt writes the Command key as "Ctrl" and Control as "Meta".
    readonly property var selectedId: backend.selection ? (JSON.parse(backend.selection)[0] || "") : ""
    readonly property bool canAct: backend.hasProject && !typing
    function nudge(dx, dy) {
        const n = inspectorNode()
        if (n) backend.moveNode(n.id, n.x + dx, n.y + dy)
    }
    function inspectorNode() { return backend.inspector && backend.inspector !== "null" ? JSON.parse(backend.inspector) : null }
    // the layer after (step 1) or before (step -1) the selected one, in the order the layers panel lists them
    function stepLayer(step) {
        const rows = (backend.layers ? JSON.parse(backend.layers) : []).filter(r => r.kind !== "page")
        if (rows.length === 0) return
        let i = rows.findIndex(r => r.id === selectedId)
        i = i < 0 ? (step > 0 ? 0 : rows.length - 1) : (i + step + rows.length) % rows.length
        if (rows[i].kind === "frame") backend.openFrame(rows[i].id)
        backend.select(rows[i].id)
    }
    function stepFrame(step) {
        if (frames.length === 0) return
        let i = frames.findIndex(f => f.id === backend.activeFrame)
        i = (i + step + frames.length) % frames.length
        backend.openFrame(frames[i].id)
        backend.select(frames[i].id)
    }

    // edit
    Shortcut { sequences: [StandardKey.Undo]; onActivated: backend.undo() }
    Shortcut { sequences: [StandardKey.Redo]; onActivated: backend.redo() }
    Shortcut { sequences: [StandardKey.Save]; onActivated: backend.saveProject() }
    Shortcut { sequences: ["Delete", "Backspace"]; enabled: win.canAct; onActivated: backend.deleteSelected() }
    Shortcut { sequences: ["Escape"]; onActivated: { win.tool = "select"; backend.select("") } }
    Shortcut { sequence: "Return"; enabled: win.canAct && win.selectedId !== ""; onActivated: { const i = win.panels.inspector; if (i) i.editText() } }
    Shortcut { sequence: "Left"; enabled: win.canAct; onActivated: win.nudge(-1, 0) }
    Shortcut { sequence: "Right"; enabled: win.canAct; onActivated: win.nudge(1, 0) }
    Shortcut { sequence: "Up"; enabled: win.canAct; onActivated: win.nudge(0, -1) }
    Shortcut { sequence: "Down"; enabled: win.canAct; onActivated: win.nudge(0, 1) }
    Shortcut { sequence: "Shift+Left"; enabled: win.canAct; onActivated: win.nudge(-10, 0) }
    Shortcut { sequence: "Shift+Right"; enabled: win.canAct; onActivated: win.nudge(10, 0) }
    Shortcut { sequence: "Shift+Up"; enabled: win.canAct; onActivated: win.nudge(0, -10) }
    Shortcut { sequence: "Shift+Down"; enabled: win.canAct; onActivated: win.nudge(0, 10) }
    Shortcut { sequence: "Tab"; enabled: win.canAct; onActivated: win.stepLayer(1) }
    Shortcut { sequence: "Shift+Tab"; enabled: win.canAct; onActivated: win.stepLayer(-1) }
    Shortcut { sequence: "Ctrl+]"; enabled: win.canAct && win.selectedId !== ""; onActivated: backend.reorder(win.selectedId, 1) }
    Shortcut { sequence: "Ctrl+["; enabled: win.canAct && win.selectedId !== ""; onActivated: backend.reorder(win.selectedId, -1) }
    Shortcut { sequence: "Ctrl+Alt+]"; enabled: win.canAct && win.selectedId !== ""; onActivated: backend.reorder(win.selectedId, 1000) }
    Shortcut { sequence: "Ctrl+Alt+["; enabled: win.canAct && win.selectedId !== ""; onActivated: backend.reorder(win.selectedId, -1000) }

    // tools
    Shortcut { sequence: "V"; enabled: win.canAct; onActivated: win.tool = "select" }
    Shortcut { sequence: "H"; enabled: win.canAct; onActivated: win.tool = "hand" }
    Shortcut { sequence: "C"; enabled: win.canAct; onActivated: win.tool = "pin" }

    // view
    Shortcut { sequences: [StandardKey.ZoomIn, "="]; enabled: !win.typing; onActivated: win.zoomBy(1.25) }
    Shortcut { sequences: [StandardKey.ZoomOut, "-"]; enabled: !win.typing; onActivated: win.zoomBy(0.8) }
    Shortcut { sequence: "Ctrl+0"; onActivated: win.zoomTo(1) }
    Shortcut { sequence: "Shift+1"; enabled: !win.typing; onActivated: win.fitCanvas() }
    Shortcut { sequence: "Shift+2"; enabled: !win.typing; onActivated: win.fitSelection() }
    Shortcut { sequence: "N"; enabled: win.canAct; onActivated: win.stepFrame(1) }
    Shortcut { sequence: "Shift+N"; enabled: win.canAct; onActivated: win.stepFrame(-1) }

    // app
    Shortcut { sequences: [StandardKey.Open]; onActivated: openDialog.open() }
    Shortcut { sequence: "Ctrl+Shift+E"; enabled: backend.hasProject; onActivated: exportDialog.openDialog() }
    Shortcut { sequence: "Ctrl+L"; enabled: backend.hasProject; onActivated: win.focusChat() }
    Shortcut { sequences: ["?", "Ctrl+/"]; enabled: !win.typing; onActivated: shortcutsDialog.open() }

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
                        { id: "select", label: "Select", key: "V", tip: "Select and move what the agent made" },
                        { id: "hand", label: "Hand", key: "H", tip: "Drag to move the view (or hold Space)" },
                        { id: "pin", label: "Note", key: "C", tip: "Pin a note for the agent on a layer or a region" }
                    ]
                    delegate: Btn {
                        required property var modelData
                        text: modelData.label
                        active: win.tool === modelData.id
                        onClicked: win.tool = modelData.id
                        ToolTip.visible: hovered
                        ToolTip.delay: 600
                        ToolTip.text: modelData.tip + " (" + modelData.key + ")"
                    }
                }
                ComboBox {
                    id: workspaceBox
                    visible: backend.hasProject
                    implicitWidth: 170
                    implicitHeight: Theme.row
                    readonly property var info: backend.workspaces ? JSON.parse(backend.workspaces) : ({ open: null, names: [] })
                    model: info.names.concat(["New workspace…"])
                    currentIndex: Math.max(0, info.names.indexOf(info.open))
                    onActivated: i => {
                        if (i === info.names.length) { newWorkspaceDialog.open(); currentIndex = Math.max(0, info.names.indexOf(info.open)) }
                        else backend.switchWorkspace(info.names[i], false)
                    }
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: "Workspaces: one per deliverable (the agent can start them too)"
                }
                ComboBox {
                    id: languageBox
                    visible: backend.hasProject
                    implicitWidth: 150
                    implicitHeight: Theme.row
                    readonly property var tags: ["", ...(backend.languages ? JSON.parse(backend.languages).map(l => l.locale) : [])]
                    model: tags.map(t => t === "" ? "Default text" : t)
                    currentIndex: Math.max(0, tags.indexOf(backend.language))
                    onActivated: i => backend.switchLanguage(tags[i])
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: "The language the canvas, the text field and exports show"
                }
                Btn {
                    visible: backend.hasProject
                    text: win.briefFormat ? win.briefFormat.width + "×" + win.briefFormat.height + (win.briefFormat.fps ? " · " + win.briefFormat.fps + " fps" : "") : "Format: agent decides"
                    onClicked: formatDialog.open()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: win.briefFormat ? win.briefFormat.label + (win.brief.chosen_by === "user" ? " (you chose it; the agent asks before changing it)" : " (the agent chose it)") : "No size chosen: the agent picks what fits. Click to choose."
                }
                Item { Layout.fillWidth: true }
                Btn { text: "Undo"; enabled: backend.canUndo; onClicked: backend.undo() }
                Btn { text: "Redo"; enabled: backend.canRedo; onClicked: backend.redo() }
                Rectangle { width: 1; height: 22; color: Theme.border; Layout.leftMargin: 6; Layout.rightMargin: 6 }
                Btn { text: "−"; implicitWidth: 30; leftPadding: 0; rightPadding: 0; onClicked: win.zoomBy(0.8) }
                Label { text: Math.round((win.canvas ? win.canvas.zoom : 1) * 100) + "%"; color: Theme.muted; Layout.preferredWidth: 44; horizontalAlignment: Text.AlignHCenter }
                Btn { text: "+"; implicitWidth: 30; leftPadding: 0; rightPadding: 0; onClicked: win.zoomBy(1.25) }
                Btn { text: "Fit"; onClicked: win.fitCanvas() }
                Rectangle { width: 1; height: 22; color: Theme.border; Layout.leftMargin: 6; Layout.rightMargin: 6 }
                Btn {
                    text: "Layout"
                    onClicked: layoutMenu.popup(parent, 0, parent.height)
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: "Show or hide panels, reset the arrangement. Drag a tab to move a panel; right-click a tab for more."
                }
                Btn { text: "Export…"; primary: true; onClicked: exportDialog.openDialog() }
                Btn {
                    text: "⋯"
                    implicitWidth: 34
                    leftPadding: 0
                    rightPadding: 0
                    onClicked: moreMenu.open()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: "Project: colors, languages, open another project, shortcuts"
                    Menu {
                        id: moreMenu
                        y: parent.height
                        MenuItem { text: "Colors…"; onTriggered: tokensDialog.open() }
                        MenuItem { text: "Languages…"; onTriggered: languagesDialog.open() }
                        MenuSeparator {}
                        MenuItem { text: "Open another project…\t⌘O"; onTriggered: openDialog.open() }
                        MenuItem { text: "Save\t⌘S"; onTriggered: backend.saveProject() }
                        MenuSeparator {}
                        MenuItem { text: "Keyboard shortcuts\t?"; onTriggered: shortcutsDialog.open() }
                    }
                }
            }
        }

        // the agent wants to change a format you chose: nothing changes until you answer
        Rectangle {
            visible: win.pendingFormat !== null
            Layout.fillWidth: true
            implicitHeight: pendingRow.implicitHeight + 16
            color: Theme.panelRaised
            border.width: 1
            border.color: Theme.agent
            RowLayout {
                id: pendingRow
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    text: win.pendingFormat ? "The agent asks to change the size from " + (win.briefFormat ? win.briefFormat.label : "?") + " to " + win.pendingFormat.format.label + " (" + win.pendingFormat.format.width + "×" + win.pendingFormat.format.height + "). Reason: " + win.pendingFormat.reason : ""
                }
                Btn { text: "Allow"; primary: true; onClicked: backend.answerFormat(true) }
                Btn { text: "Keep mine"; onClicked: backend.answerFormat(false) }
            }
        }

        // where the panels are: side by side, stacked, as tabs, or in windows of their own
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Repeater {
                model: dock.layout ? [dock.mainRootId] : []
                delegate: DockNode {
                    required property int modelData
                    anchors.fill: parent
                    host: dock
                    nodeId: modelData
                    windowId: 0
                }
            }
        }
    }

    // ---------- panels and windows ----------
    // What each panel is and what it is wired to. The dock decides where each one is shown.
    Component { id: layersPanel; LayersPanel { editor: backend } }
    Component {
        id: canvasPanel
        ColumnLayout {
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
                    model: framesModel
                    spacing: 2
                    clip: true
                    delegate: Btn {
                        required property string json
                        readonly property var modelData: JSON.parse(json)
                        text: modelData.name + "  " + Math.round(modelData.width) + "×" + Math.round(modelData.height)
                        active: modelData.id === backend.activeFrame
                        height: 34
                        onClicked: { backend.openFrame(modelData.id); backend.select(modelData.id) }
                    }
                }
            }
            CanvasView {
                id: canvasView
                editor: backend
                tool: win.tool
                Layout.fillWidth: true
                Layout.fillHeight: true
                onToolFinished: win.tool = "select"
                onRequestPin: (nodeId, x, y, w, h) => { win.raise(); pinDialog.openFor(nodeId, x, y, w, h) }
                onNoteClicked: id => win.showPanel("notes")
                Component.onCompleted: win.canvas = canvasView
                Component.onDestruction: if (win.canvas === canvasView) win.canvas = null
            }
        }
    }
    Component {
        id: inspectorPanel
        InspectorPanel {
            id: inspectorView
            editor: backend
            onRequestPin: nodeId => { win.raise(); pinDialog.openFor(nodeId, 0, 0, 0, 0) }
            onRequestReplaceImage: nodeId => { imageDialog.targetId = nodeId; imageDialog.open() }
            Component.onCompleted: win.panels.inspector = inspectorView
            Component.onDestruction: if (win.panels.inspector === inspectorView) win.panels.inspector = null
        }
    }
    Component { id: notesPanel; NotesPanel { editor: backend } }
    Component { id: historyPanel; HistoryPanel { editor: backend } }
    Component { id: memoryPanel; MemoryPanel { editor: backend } }
    Component { id: videoPanel; VideoPanel { editor: backend } }
    Component { id: connectPanel; ConnectPanel { editor: backend; onToast: text => win.toast(text, false) } }
    Component {
        id: chatPanel
        ChatPanel {
            id: chatView
            editor: backend
            Component.onCompleted: win.panels.chat = chatView
            Component.onDestruction: if (win.panels.chat === chatView) win.panels.chat = null
        }
    }

    Dock {
        id: dock
        editor: backend
        mainWindow: win
        components: ({
            layers: layersPanel, canvas: canvasPanel, inspector: inspectorPanel, notes: notesPanel,
            history: historyPanel, memory: memoryPanel, video: videoPanel, connect: connectPanel, chat: chatPanel
        })
    }

    // a panel taken out of the window lives in a window of its own; closing it puts it back
    Instantiator {
        id: floatingWindows
        model: backend.hasProject ? dock.windowIds : []
        delegate: Window {
            id: fw
            required property int modelData
            readonly property int windowId: modelData
            readonly property var spec: dock.window(modelData)
            readonly property int rootId: spec ? spec.root.id : 0
            visible: spec !== null
            color: Theme.canvasBg
            title: dock.windowTitle(modelData)
            minimumWidth: 240
            minimumHeight: 200
            Component.onCompleted: {
                if (!spec) return
                x = spec.x; y = spec.y; width = spec.width; height = spec.height
            }
            // keep where the user left it (after they stop moving it)
            Timer {
                id: keep
                interval: 500
                onTriggered: dock.setGeometry(fw.windowId, fw.x, fw.y, fw.width, fw.height)
            }
            onXChanged: keep.restart()
            onYChanged: keep.restart()
            onWidthChanged: keep.restart()
            onHeightChanged: keep.restart()
            onClosing: close => { close.accepted = true; Qt.callLater(() => dock.dockWindow(fw.windowId)) }
            Repeater {
                model: fw.rootId > 0 ? [fw.rootId] : []
                delegate: DockNode {
                    required property int modelData
                    anchors.fill: parent
                    host: dock
                    nodeId: modelData
                    windowId: fw.windowId
                }
            }
        }
    }

    Menu {
        id: layoutMenu
        Instantiator {
            model: dock.panelOrder
            delegate: MenuItem {
                required property string modelData
                text: dock.panelTitles[modelData]
                checkable: true
                checked: dock.hidden.indexOf(modelData) < 0
                onTriggered: { if (dock.hidden.indexOf(modelData) >= 0) dock.show(modelData); else dock.hide(modelData) }
            }
            onObjectAdded: (i, o) => layoutMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => layoutMenu.removeItem(o)
        }
        MenuSeparator {}
        MenuItem { text: "Put every window back"; enabled: dock.windowIds.length > 0; onTriggered: { for (const id of dock.windowIds) dock.dockWindow(id) } }
        MenuItem { text: "Reset the layout"; onTriggered: dock.reset() }
    }

    // ---------- welcome ----------
    Rectangle {
        anchors.fill: parent
        visible: !backend.hasProject
        color: Theme.canvasBg
        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(560, parent.width - 60)
            spacing: 10
            Image {
                source: "qrc:/qt/qml/org/opendesigner/app/qml/logo.svg"
                sourceSize.height: 90
                Layout.preferredHeight: 90
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
                text: "Your agent makes the assets and videos; you guide it by pointing."
                color: Theme.muted
                font.pixelSize: 14
                Layout.alignment: Qt.AlignHCenter
            }
            StartPanel {
                Layout.fillWidth: true
                Layout.topMargin: 10
                editor: backend
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

    // Open another project (or start the agent on one): the same panel as the start screen.
    Modal {
        id: openDialog
        title: "Open a project"
        width: 580
        standardButtons: Dialog.Close
        contentItem: StartPanel { editor: backend; onStarted: openDialog.close() }
    }
    FileDialog {
        id: imageDialog
        property string targetId: ""
        title: targetId !== "" ? "Choose the new picture" : "Add an image"
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp)"]
        onAccepted: {
            if (targetId !== "") backend.replaceImage(selectedFile.toString(), targetId)
            else backend.importImage(selectedFile.toString())
            targetId = ""
        }
        onRejected: targetId = ""
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
            win.showPanel("notes")
        }
    }

    Modal {
        id: languagesDialog
        title: "Languages"
        width: 520
        height: 440
        standardButtons: Dialog.Close
        readonly property var rows: backend.languages ? JSON.parse(backend.languages) : []
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "Adding a language copies every text layer's current text as a starting point. Pick the language in the toolbar, then edit the text of each layer; exports use the language you picked. Agents can translate too, and every change can be undone."
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 4
                model: languagesDialog.rows
                delegate: RowLayout {
                    id: langRow
                    required property var modelData
                    width: ListView.view.width
                    spacing: 8
                    Label { text: langRow.modelData.locale; color: Theme.text; font.bold: true; Layout.preferredWidth: 80 }
                    Label {
                        Layout.fillWidth: true
                        color: Theme.muted
                        font.pixelSize: 12
                        text: langRow.modelData.translated + " translated, " + langRow.modelData.missing.length + " still the default"
                    }
                    Btn { text: "Use"; onClicked: backend.switchLanguage(langRow.modelData.locale) }
                    Btn { text: "Remove"; onClicked: backend.removeLanguage(langRow.modelData.locale) }
                }
            }
            RowLayout {
                spacing: 8
                Field { id: newLanguage; Layout.fillWidth: true; placeholderText: "Language tag, for example ja or pt-BR" }
                Btn {
                    text: "Add"
                    primary: true
                    enabled: newLanguage.text.trim() !== ""
                    onClicked: {
                        backend.addLanguage(newLanguage.text)
                        if (!backend.messageIsError) newLanguage.text = ""
                    }
                }
            }
        }
    }

    Modal {
        id: tokensDialog
        title: "Colors (design tokens)"
        width: 520
        height: 460
        standardButtons: Dialog.Close
        readonly property var rows: backend.tokens ? JSON.parse(backend.tokens) : []
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "Use a color as {name} in a fill or stroke. Changing a color here recolors everything that uses it, and agents can change them too; every change can be undone."
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 4
                model: tokensDialog.rows
                delegate: RowLayout {
                    id: tokenRow
                    required property var modelData
                    width: ListView.view.width
                    spacing: 8
                    Rectangle { Layout.preferredWidth: 22; Layout.preferredHeight: 22; radius: 4; color: tokenRow.modelData.value; border.width: 1; border.color: Theme.border }
                    Label { text: tokenRow.modelData.name; color: Theme.text; font.pixelSize: Theme.fontSize; Layout.fillWidth: true; elide: Text.ElideRight }
                    Label { text: tokenRow.modelData.used_by + " used"; color: Theme.muted; font.pixelSize: 11 }
                    Field {
                        Layout.preferredWidth: 100
                        text: tokenRow.modelData.value
                        onCommitted: v => { if (v !== tokenRow.modelData.value) backend.setToken(tokenRow.modelData.name, v) }
                    }
                    Btn {
                        text: "Remove"
                        enabled: tokenRow.modelData.used_by === 0
                        onClicked: backend.removeToken(tokenRow.modelData.name)
                    }
                }
            }
            RowLayout {
                spacing: 8
                Field { id: newTokenName; Layout.fillWidth: true; placeholderText: "new.name" }
                Field { id: newTokenValue; Layout.preferredWidth: 100; placeholderText: "#rrggbb" }
                Btn {
                    text: "Add"
                    primary: true
                    enabled: newTokenName.text.trim() !== "" && newTokenValue.text.trim() !== ""
                    onClicked: {
                        backend.setToken(newTokenName.text, newTokenValue.text)
                        if (!backend.messageIsError) { newTokenName.text = ""; newTokenValue.text = "" }
                    }
                }
            }
        }
    }

    Modal {
        id: exportDialog
        title: "Export"
        width: 560
        standardButtons: Dialog.Ok | Dialog.Cancel
        property var presets: []
        property string mode: "frame"
        function openDialog() {
            presets = JSON.parse(backend.framePresets()).filter(p => p.id.indexOf("web-") !== 0)
            mode = "frame"
            fileField.text = "shot-01.png"
            presetBox.currentIndex = 0
            slicesBox.value = 3
            open()
        }
        contentItem: ColumnLayout {
            spacing: 8
            Label { text: "What to export"; color: Theme.muted; font.pixelSize: 12 }
            RadioButton {
                text: "The open frame"
                checked: exportDialog.mode === "frame"
                onClicked: { exportDialog.mode = "frame"; fileField.text = "shot-01.png" }
            }
            RadioButton {
                text: "Every frame on this page, as a set"
                checked: exportDialog.mode === "set"
                onClicked: { exportDialog.mode = "set"; fileField.text = "shot" }
            }
            RowLayout {
                RadioButton {
                    text: "Cut the open frame into"
                    checked: exportDialog.mode === "panorama"
                    onClicked: { exportDialog.mode = "panorama"; fileField.text = "shot" }
                }
                SpinBox { id: slicesBox; from: 2; to: 50; value: 3; editable: true; enabled: exportDialog.mode === "panorama" }
                Label { text: "equal slides (a panorama)"; color: Theme.text }
            }
            Label { text: "Check against"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                id: presetBox
                Layout.fillWidth: true
                model: ["No store preset"].concat(exportDialog.presets.map(p => p.label))
            }
            Label {
                text: exportDialog.mode === "frame" ? "File name (saved in the project's exports folder)"
                                                    : "Name stem: shot gives shot-01.png, shot-02.png, ..."
                color: Theme.muted
                font.pixelSize: 12
            }
            Field { id: fileField; Layout.fillWidth: true }
            CheckBox {
                id: alphaBox
                text: "Keep the transparency (a frame with no background stays see-through)"
                font.pixelSize: 12
                visible: exportDialog.mode !== "panorama"
                contentItem: Text { text: parent.text; font: parent.font; color: Theme.text; leftPadding: parent.indicator.width + 4; verticalAlignment: Text.AlignVCenter }
                ToolTip.visible: hovered
                ToolTip.text: "Stores reject images with an alpha channel, so this is only for pictures you will put on something else."
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "With a store chosen, every image and the number of images must meet its rules or nothing is written; you are told which rule each file breaks. A frame with no background is transparent: the file is white there unless you keep the transparency."
            }
        }
        onAccepted: {
            const preset = presetBox.currentIndex > 0 ? exportDialog.presets[presetBox.currentIndex - 1].id : ""
            backend.exportAssets(exportDialog.mode, preset, fileField.text.trim(), slicesBox.value, alphaBox.checked)
        }
    }

    // ---------- work in progress ----------
    // Anything that takes a while (a video export, copying a large video, starting an agent)
    // shows here with a bar, or a spinner when it cannot say how far along it is.
    JsonModel { id: jobsModel }
    readonly property var jobs: backend.jobs ? JSON.parse(backend.jobs) : []
    onJobsChanged: jobsModel.sync(jobs)
    Column {
        id: jobStrip
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 16
        width: Math.min(380, parent.width - 32)
        spacing: 6
        z: 5
        Repeater {
            model: jobsModel
            delegate: Rectangle {
                required property string json
                readonly property var job: JSON.parse(json)
                width: jobStrip.width
                height: 54
                radius: Theme.radius
                color: Theme.panelRaised
                border.width: 1
                border.color: Theme.border
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        BusyIndicator { running: job.total === 0; visible: job.total === 0; implicitWidth: 18; implicitHeight: 18 }
                        Label { text: job.label; color: Theme.text; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
                        Label { visible: job.total > 0; text: Math.round(100 * job.done / job.total) + "%"; color: Theme.muted; font.pixelSize: 11 }
                        Btn { visible: job.cancellable; text: "Stop"; implicitHeight: 22; onClicked: backend.cancelJob(job.id) }
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        indeterminate: job.total === 0
                        from: 0
                        to: Math.max(1, job.total)
                        value: job.done
                    }
                }
            }
        }
    }

    Modal {
        id: formatDialog
        title: "Size and format"
        width: 640
        standardButtons: Dialog.Close
        contentItem: ScrollView {
            id: formatScroll
            implicitHeight: Math.min(formatCol.implicitHeight + 8, 520)
            contentWidth: availableWidth
            contentHeight: formatCol.implicitHeight + 8
            clip: true
            ColumnLayout {
                id: formatCol
                width: formatScroll.availableWidth
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 12
                    text: win.briefFormat ? "Now: " + win.briefFormat.label + " " + win.briefFormat.width + "×" + win.briefFormat.height + (win.brief.chosen_by === "user" ? ". You chose it, so the agent asks before changing it." : ". The agent chose it and may change it.") : "Nothing chosen: the agent picks what fits the work and tells you. Choose a size to make it keep to it."
                }
                FormatPicker {
                    Layout.fillWidth: true
                    editor: backend
                    current: win.briefFormat && win.brief.chosen_by === "user" ? (win.briefFormat.id || "custom") : ""
                    onPicked: f => backend.setFormat(f ? (f.id || "") : "", f ? (f.kind || "") : "", f ? (f.width || 0) : 0, f ? (f.height || 0) : 0, f ? (f.fps || 0) : 0)
                }
            }
        }
    }

    Modal {
        id: newWorkspaceDialog
        title: "New workspace"
        width: 440
        standardButtons: Dialog.Ok | Dialog.Cancel
        onOpened: { workspaceName.text = ""; workspaceName.forceActiveFocus() }
        contentItem: ColumnLayout {
            Label { text: "A separate design for one deliverable (an app store set, a reel, a landing page)."; color: Theme.muted; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Field { id: workspaceName; Layout.fillWidth: true; placeholderText: "app store" }
        }
        onAccepted: if (workspaceName.text.trim() !== "") backend.switchWorkspace(workspaceName.text.trim(), true)
    }

    // ---------- keyboard shortcuts ----------
    Modal {
        id: shortcutsDialog
        title: "Keyboard shortcuts"
        width: 560
        standardButtons: Dialog.Close
        readonly property var groups: [
            { name: "View", rows: [
                ["Pan", "two fingers on the trackpad, or hold Space and drag, or H"],
                ["Zoom", "pinch, or ⌘ + scroll, or ⌥ + scroll"],
                ["Zoom in / out", "⌘+  ⌘−   or   =  −"],
                ["Fit the frame / the selection", "⇧1   /   ⇧2"],
                ["Zoom to 100%", "⌘0"],
                ["Next / previous frame", "N   /   ⇧N"]] },
            { name: "Pointing", rows: [
                ["Select", "V   (click a layer on the canvas)"],
                ["Pin a note for the agent", "C   (click a layer, or drag a region)"],
                ["Next / previous layer", "Tab   /   ⇧Tab"],
                ["Edit the selected text", "Return"],
                ["Deselect", "Esc"]] },
            { name: "Edit", rows: [
                ["Undo / redo", "⌘Z   /   ⇧⌘Z"],
                ["Move the selection", "arrows 1 px,  ⇧ arrows 10 px"],
                ["Bring forward / send back", "⌘]   /   ⌘["],
                ["Bring to front / send to back", "⌥⌘]   /   ⌥⌘["],
                ["Delete", "⌫"]] },
            { name: "App", rows: [
                ["Talk to the agent", "⌘L"],
                ["Export", "⇧⌘E"],
                ["Open a project", "⌘O"],
                ["Save", "⌘S"],
                ["Show this list", "?   or   ⌘/"]] }
        ]
        contentItem: ColumnLayout {
            spacing: 10
            Repeater {
                model: shortcutsDialog.groups
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 3
                    Label { text: modelData.name; color: Theme.accent; font.pixelSize: 11; font.bold: true }
                    Repeater {
                        model: modelData.rows
                        delegate: RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            Label { text: modelData[0]; color: Theme.text; font.pixelSize: 12; Layout.preferredWidth: 220 }
                            Label { text: modelData[1]; color: Theme.muted; font.pixelSize: 12; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                        }
                    }
                }
            }
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

    // The first time the app runs on a computer that has an agent but not our skill, say so and
    // show exactly what would be written. Nothing is written without a click.
    Dialog {
        id: offer
        property var rows: []
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, Overlay.overlay ? Overlay.overlay.width - 40 : 560)
        title: "Teach your coding agents about OpenDesigner?"
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: offer.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14; wrapMode: Text.WordWrap }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "OpenDesigner found coding agents on this computer. A short skill (a guide and the store rules) lets them work in your projects. These files would be written, and nothing else is touched; you can remove them again from the Connect tab."
            }
            Repeater {
                model: offer.rows
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Label { text: modelData.label + " — " + modelData.readers.join(", "); color: Theme.text; font.pixelSize: 12; font.bold: true; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                    Repeater {
                        model: modelData.files
                        Label { required property string modelData; text: modelData; color: Theme.muted; font.pixelSize: 11; elide: Text.ElideMiddle; Layout.fillWidth: true }
                    }
                }
            }
        }
        footer: RowLayout {
            spacing: 8
            Item { Layout.fillWidth: true }
            Btn { text: "Don't ask again"; onClicked: { backend.answerSkillOffer(false); offer.close() } }
            Btn { text: "Not now"; onClicked: offer.close() }
            Btn { text: "Install"; primary: true; onClicked: { backend.answerSkillOffer(true); offer.close() } }
            Item { implicitWidth: 6 }
        }
    }

    Component.onCompleted: {
        backend.layoutLoad(selfTest.folder === "")
        if (selfTest.folder !== "" || !backend.skillOfferPending()) return
        offer.rows = JSON.parse(backend.skillStatus()).filter(r => r.detected && r.state === "not installed")
        offer.open()
    }

    SelfTest { id: selfTest; editor: backend; canvas: win.canvas; dock: dock; grabTarget: shell }
}
