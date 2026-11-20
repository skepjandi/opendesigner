import QtQuick
import QtQuick.Controls
import org.opendesigner.app

// The application menu bar. Qt 6.8+ shows Qt Quick Controls menus in the platform's menu bar
// (the top-of-screen bar on macOS); where the platform has none the bar is drawn in the window.
// Every Ctrl/Cmd shortcut of the window lives here. Single-key shortcuts (V, H, C, arrows, Delete,
// ...) stay in main.qml because they must stay off while a text field has focus.
MenuBar {
    id: bar

    required property var win
    required property var dock
    required property var editor
    required property var dialogs

    readonly property bool project: editor.hasProject
    readonly property bool typing: win.typing
    readonly property int selected: editor.selection ? JSON.parse(editor.selection).length : 0
    readonly property bool hasSelection: project && selected > 0 && !typing
    readonly property bool canPaste: project && editor.canPaste === true
    readonly property bool canGroup: project && editor.canGroup === true
    // macOS adds its own Enter Full Screen item to the View menu
    readonly property bool ownFullScreen: Qt.platform.os !== "osx"
    readonly property bool canUngroup: project && editor.canUngroup === true
    readonly property bool canArrange: hasSelection
    readonly property bool canDistribute: project && selected >= 3 && !typing
    readonly property real stackGap: 16

    // one menu entry: its shortcut, enabled state and check mark live on an Action, which is what
    // the native menu reads
    component Cmd: MenuItem {
        id: item
        property string label
        property var keys: []
        property bool ok: true
        property bool toggle: false
        property bool on: false
        signal run()
        action: Action {
            id: act
            text: item.label
            shortcut: item.keys
            enabled: item.ok
            checkable: item.toggle
            onTriggered: { item.run(); if (item.toggle) act.checked = item.on }
        }
        Binding { target: act; property: "checked"; value: item.on; when: item.toggle }
    }

    // the focused text field takes the standard editing commands; otherwise they act on the canvas
    function edit(name, fallback) {
        const f = win.typing ? win.activeFocusItem : null
        if (f && f[name]) f[name](); else fallback()
    }
    function togglePanel(id) { if (dock.hidden.indexOf(id) >= 0) dock.show(id); else dock.hide(id) }

    Menu {
        title: "File"
        onAboutToShow: recentMenu.refresh()
        Cmd { label: "New Project…"; keys: [StandardKey.New]; onRun: bar.dialogs.openProject.open() }
        Cmd { label: "Open Project…"; keys: [StandardKey.Open]; onRun: bar.dialogs.openProject.open() }
        Menu {
            id: recentMenu
            title: "Open Recent"
            enabled: recentItems.count > 0
            property var list: []
            function refresh() { list = JSON.parse(bar.editor.recentProjects()) }
            Component.onCompleted: refresh()
            Instantiator {
                id: recentItems
                model: recentMenu.list
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.name
                    onTriggered: bar.editor.openProject(modelData.folder)
                }
                onObjectAdded: (i, o) => recentMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => recentMenu.removeItem(o)
            }
        }
        MenuSeparator {}
        Cmd { label: "Save"; keys: [StandardKey.Save]; ok: bar.project; onRun: bar.editor.saveProject() }
        Cmd { label: "Export…"; keys: ["Ctrl+Shift+E"]; ok: bar.project; onRun: bar.dialogs.exportAssets.openDialog() }
        MenuSeparator {}
        Cmd { label: "Quit"; keys: [StandardKey.Quit]; onRun: Qt.quit() }
    }

    Menu {
        title: "Edit"
        Cmd { label: "Undo"; keys: [StandardKey.Undo]; ok: bar.typing || bar.editor.canUndo; onRun: bar.edit("undo", () => bar.editor.undo()) }
        Cmd { label: "Redo"; keys: [StandardKey.Redo]; ok: bar.typing || bar.editor.canRedo; onRun: bar.edit("redo", () => bar.editor.redo()) }
        MenuSeparator {}
        Cmd { label: "Cut"; keys: [StandardKey.Cut]; ok: bar.typing; onRun: bar.edit("cut", () => {}) }
        Cmd { label: "Copy"; keys: [StandardKey.Copy]; ok: bar.typing || bar.hasSelection; onRun: bar.edit("copy", () => bar.editor.copySelected()) }
        Cmd { label: "Paste"; keys: [StandardKey.Paste]; ok: bar.typing || bar.canPaste; onRun: bar.edit("paste", () => bar.editor.pasteClipboard()) }
        Cmd { label: "Duplicate"; keys: ["Ctrl+D"]; ok: bar.hasSelection; onRun: bar.editor.duplicateSelected() }
        Cmd { label: "Delete  (⌫)"; ok: bar.hasSelection; onRun: bar.editor.deleteSelected() }
        MenuSeparator {}
        Cmd { label: "Select All"; keys: [StandardKey.SelectAll]; ok: bar.typing || bar.project; onRun: bar.edit("selectAll", () => bar.editor.selectAll()) }
        Cmd { label: "Group"; keys: ["Ctrl+G"]; ok: bar.canGroup && !bar.typing; onRun: bar.editor.groupSelected() }
        Cmd { label: "Ungroup"; keys: ["Ctrl+Shift+G"]; ok: bar.canUngroup && !bar.typing; onRun: bar.editor.ungroupSelected() }
    }

    Menu {
        title: "Insert"
        Cmd { label: "Select  (V)"; ok: bar.project; toggle: true; on: bar.win.tool === "select"; onRun: bar.win.tool = "select" }
        Cmd { label: "Hand  (H)"; ok: bar.project; toggle: true; on: bar.win.tool === "hand"; onRun: bar.win.tool = "hand" }
        Cmd { label: "Note  (C)"; ok: bar.project; toggle: true; on: bar.win.tool === "pin"; onRun: bar.win.tool = "pin" }
        Cmd { label: "Rectangle  (R)"; ok: bar.project; toggle: true; on: bar.win.tool === "rect"; onRun: bar.win.tool = "rect" }
        Cmd { label: "Ellipse  (O)"; ok: bar.project; toggle: true; on: bar.win.tool === "ellipse"; onRun: bar.win.tool = "ellipse" }
        Cmd { label: "Text  (T)"; ok: bar.project; toggle: true; on: bar.win.tool === "text"; onRun: bar.win.tool = "text" }
        Cmd { label: "Frame  (F)"; ok: bar.project; toggle: true; on: bar.win.tool === "frame"; onRun: bar.win.tool = "frame" }
        MenuSeparator {}
        Menu {
            id: deviceMenu
            title: "Add Device Frame"
            enabled: bar.project
            property var kinds: []
            Component.onCompleted: kinds = JSON.parse(bar.editor.deviceKinds())
            Instantiator {
                model: deviceMenu.kinds
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.label
                    onTriggered: bar.editor.addDevice(modelData.id)
                }
                onObjectAdded: (i, o) => deviceMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => deviceMenu.removeItem(o)
            }
        }
        Menu {
            id: device3dMenu
            title: "Add 3D Device"
            enabled: bar.project
            property var models: []
            Component.onCompleted: models = JSON.parse(bar.editor.deviceModels())
            Instantiator {
                model: device3dMenu.models
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.label
                    onTriggered: bar.editor.addDevice3d(modelData.id)
                }
                onObjectAdded: (i, o) => device3dMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => device3dMenu.removeItem(o)
            }
        }
        Cmd { label: "Import Image…"; ok: bar.project; onRun: bar.dialogs.image.open() }
        MenuSeparator {}
        Cmd { label: "Add Language…"; ok: bar.project; onRun: bar.dialogs.languages.open() }
        Cmd { label: "Colors…"; ok: bar.project; onRun: bar.dialogs.tokens.open() }
        Cmd { label: "Languages…"; ok: bar.project; onRun: bar.dialogs.languages.open() }
    }

    Menu {
        title: "Arrange"
        Cmd { label: "Bring Forward"; keys: ["Ctrl+]"]; ok: bar.hasSelection; onRun: bar.editor.bringSelected(1) }
        Cmd { label: "Send Backward"; keys: ["Ctrl+["]; ok: bar.hasSelection; onRun: bar.editor.bringSelected(-1) }
        Cmd { label: "Bring to Front"; keys: ["Ctrl+Alt+]"]; ok: bar.hasSelection; onRun: bar.editor.bringSelected(1000) }
        Cmd { label: "Send to Back"; keys: ["Ctrl+Alt+["]; ok: bar.hasSelection; onRun: bar.editor.bringSelected(-1000) }
        MenuSeparator {}
        Cmd { label: "Align Left"; ok: bar.canArrange; onRun: bar.editor.alignSelected("left") }
        Cmd { label: "Align Horizontal Centers"; ok: bar.canArrange; onRun: bar.editor.alignSelected("hcenter") }
        Cmd { label: "Align Right"; ok: bar.canArrange; onRun: bar.editor.alignSelected("right") }
        Cmd { label: "Align Top"; ok: bar.canArrange; onRun: bar.editor.alignSelected("top") }
        Cmd { label: "Align Vertical Middles"; ok: bar.canArrange; onRun: bar.editor.alignSelected("vmiddle") }
        Cmd { label: "Align Bottom"; ok: bar.canArrange; onRun: bar.editor.alignSelected("bottom") }
        MenuSeparator {}
        Cmd { label: "Distribute Horizontally"; ok: bar.canDistribute; onRun: bar.editor.distributeSelected("h") }
        Cmd { label: "Distribute Vertically"; ok: bar.canDistribute; onRun: bar.editor.distributeSelected("v") }
        MenuSeparator {}
        Cmd { label: "Stack in a Row (16 px gap)"; ok: bar.canArrange; onRun: bar.editor.stackSelected("row", bar.stackGap) }
        Cmd { label: "Stack in a Column (16 px gap)"; ok: bar.canArrange; onRun: bar.editor.stackSelected("column", bar.stackGap) }
    }

    Menu {
        id: viewMenu
        title: "View"
        Cmd { label: "Zoom In"; keys: [StandardKey.ZoomIn]; ok: bar.project; onRun: bar.win.zoomBy(1.25) }
        Cmd { label: "Zoom Out"; keys: [StandardKey.ZoomOut]; ok: bar.project; onRun: bar.win.zoomBy(0.8) }
        Cmd { label: "Actual Size (100%)"; keys: ["Ctrl+0"]; ok: bar.project; onRun: bar.win.zoomTo(1) }
        Cmd { label: "Fit Frame  (⇧1)"; ok: bar.project; onRun: bar.win.fitCanvas() }
        Cmd { label: "Fit Selection  (⇧2)"; ok: bar.hasSelection; onRun: bar.win.fitSelection() }
        MenuSeparator {}
        Cmd { label: "Hide Editor Overlays  (⇧H)"; ok: bar.project; toggle: true; on: bar.win.overlaysHidden; onRun: bar.win.overlaysHidden = !bar.win.overlaysHidden }
        Cmd { label: "Rulers"; ok: bar.project; toggle: true; on: bar.win.rulersVisible; onRun: bar.win.rulersVisible = !bar.win.rulersVisible }
        Cmd { label: "Snapping"; ok: bar.project; toggle: true; on: bar.win.snapEnabled; onRun: bar.win.snapEnabled = !bar.win.snapEnabled }
        MenuSeparator {}
        Menu {
            id: panelsMenu
            title: "Panels"
            Instantiator {
                model: bar.dock.panelOrder
                delegate: MenuItem {
                    required property string modelData
                    text: bar.dock.panelTitles[modelData]
                    checkable: true
                    checked: bar.dock.hidden.indexOf(modelData) < 0
                    onTriggered: { bar.togglePanel(modelData); checked = Qt.binding(() => bar.dock.hidden.indexOf(modelData) < 0) }
                }
                onObjectAdded: (i, o) => panelsMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => panelsMenu.removeItem(o)
            }
        }
        Cmd { label: "Put Every Window Back"; ok: bar.dock.windowIds.length > 0; onRun: { for (const id of bar.dock.windowIds) bar.dock.dockWindow(id) } }
        Cmd { label: "Reset Layout"; onRun: bar.dock.reset() }
        Instantiator {
            model: bar.ownFullScreen ? 1 : 0
            delegate: MenuSeparator {}
            onObjectAdded: (i, o) => viewMenu.addItem(o)
            onObjectRemoved: (i, o) => viewMenu.removeItem(o)
        }
        Instantiator {
            model: bar.ownFullScreen ? 1 : 0
            delegate: Cmd {
                label: bar.win.visibility === Window.FullScreen ? "Exit Full Screen" : "Enter Full Screen"
                keys: [StandardKey.FullScreen]
                onRun: bar.win.visibility = bar.win.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen
            }
            onObjectAdded: (i, o) => viewMenu.addItem(o)
            onObjectRemoved: (i, o) => viewMenu.removeItem(o)
        }
    }

    Menu {
        title: "Agent"
        Cmd { label: "Focus Chat"; keys: ["Ctrl+L"]; ok: bar.project; onRun: bar.win.focusChat() }
        Cmd { label: "Connect Agent…"; ok: bar.project; onRun: bar.win.showPanel("connect") }
        MenuSeparator {}
        Cmd { label: "Variants"; ok: bar.project; onRun: bar.win.showPanel("variants") }
        Cmd { label: "Preview"; ok: bar.project; onRun: bar.win.showPanel("preview") }
        Cmd { label: "Checkpoints and History"; ok: bar.project; onRun: bar.win.showPanel("history") }
    }

    Menu {
        title: "Window"
        Cmd { label: "Minimize"; keys: ["Ctrl+M"]; onRun: bar.win.showMinimized() }
        Cmd { label: "Zoom"; onRun: bar.win.visibility = bar.win.visibility === Window.Maximized ? Window.Windowed : Window.Maximized }
    }

    Menu {
        title: "Help"
        Cmd { label: "Keyboard Shortcuts  (?)"; keys: ["Ctrl+/"]; onRun: bar.dialogs.shortcuts.open() }
        Cmd { label: "Report an Issue…"; onRun: Qt.openUrlExternally("https://github.com/skepjandi/opendesigner/issues/new/choose") }
    }
}
