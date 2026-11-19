import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Where to work, which agent to start and what to ask it first. Used on launch and from the
// project menu. The folder is usually a repository: the agent starts there (so it can run the
// app and take screenshots) and the design lives in an `opendesigner/` folder inside it.
ScrollView {
    id: root
    required property var editor
    signal started()

    property string folderUrl: ""
    property var info: null
    readonly property var presets: JSON.parse(editor.chatPresets())
    readonly property var recent: JSON.parse(editor.recentProjects())
    readonly property bool agentChosen: agentBox.currentIndex >= 0 && presets[agentBox.currentIndex].command !== "" || customCommand.text.trim() !== ""
    readonly property string command: presets[agentBox.currentIndex].command !== "" ? presets[agentBox.currentIndex].command : customCommand.text.trim()

    // the agent's models and efforts, read by starting it briefly (the editor fills `agentProbe`)
    readonly property var probe: editor.agentProbe ? JSON.parse(editor.agentProbe) : ({})
    readonly property bool probeMatches: probe.command === root.command
    readonly property var agentConfig: probeMatches && probe.config ? probe.config : []
    readonly property var modelOption: agentConfig.find(o => o.category === "model") || null
    readonly property var effortOption: agentConfig.find(o => o.category === "thought_level") || null
    property string modelValue: ""
    property string effortValue: ""
    function readOptions() {
        modelValue = ""; effortValue = ""
        if (agentChosen && command !== "") editor.probeAgent(command)
    }
    Timer { id: probeTimer; interval: 500; onTriggered: root.readOptions() }
    onCommandChanged: probeTimer.restart()

    function choose(url) {
        folderUrl = url
        info = JSON.parse(editor.inspectFolder(url))
    }

    Component.onCompleted: {
        const first = presets.findIndex(p => p.found)
        agentBox.currentIndex = first >= 0 ? first : 0
        const last = editor.chatLastCommand()
        const at = presets.findIndex(p => p.command === last)
        if (last !== "" && at >= 0) agentBox.currentIndex = at
        if (recent.length > 0) choose("file://" + recent[0].folder)
    }

    DirectionPicker {
        id: direction
        editor: root.editor
        onPicked: text => {
            if (text === "") return
            const current = prompt.text.trim()
            prompt.text = current === "" ? text : current + "\n\n" + text
        }
    }

    FolderDialog {
        id: folderDialog
        title: "Choose the folder you are working in"
        onAccepted: root.choose(selectedFolder.toString())
    }

    readonly property var existing: info && info.found === "workspaces" ? info.workspaces : []
    readonly property bool direct: info !== null && info.found === "project"
    property bool makeNew: existing.length === 0
    // optional: what the work is for (null: the agent decides)
    property var format: null
    readonly property string formatLabel: {
        if (!format) return "The agent decides"
        if (format.id) {
            for (const s of JSON.parse(editor.formats())) for (const f of s.formats) if (f.id === format.id) return f.label + "  " + f.width + "×" + f.height
        }
        return "Custom " + format.width + "×" + format.height + (format.fps ? " · " + format.fps + " fps" : "")
    }
    clip: true
    implicitWidth: 540
    contentWidth: availableWidth
    contentHeight: column.implicitHeight + 40
    implicitHeight: Math.min(column.implicitHeight + 40, 640)
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    ColumnLayout {
        id: column
        width: root.availableWidth
        spacing: 12

        Label { text: "Where are you working?"; color: Theme.text; font.pixelSize: 15; font.bold: true }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 12
            text: "Choose the folder of your project (a repository, say). The agent starts there, so it can run your app and take screenshots; the designs, notes and exports are kept in an opendesigner folder inside it."
        }
        RowLayout {
            Layout.fillWidth: true
            Label {
                Layout.fillWidth: true
                text: root.info ? root.editor.localPath(root.folderUrl) : "No folder chosen"
                color: root.info ? Theme.text : Theme.muted
                elide: Text.ElideMiddle
            }
            Btn { text: "Choose folder…"; primary: !root.info; onClicked: folderDialog.open() }
        }
        // One workspace per deliverable: an app store set, a reel, a landing page. Pick one that
        // exists or name a new one.
                    RowLayout {
            Layout.fillWidth: true
            visible: root.info !== null && !root.direct
            Label { text: "Workspace"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                id: workspaceBox
                Layout.preferredWidth: 180
                visible: root.existing.length > 0
                model: root.existing.concat(["New workspace…"])
                onActivated: i => root.makeNew = i === root.existing.length
                onModelChanged: { currentIndex = 0; root.makeNew = root.existing.length === 0 }
            }
            Field {
                id: newName
                Layout.fillWidth: true
                visible: root.makeNew
                placeholderText: "name it after what it is, for example app store or instagram reel"
            }
        }
        Label {
            Layout.fillWidth: true
            visible: root.info !== null
            wrapMode: Text.WordWrap
            font.pixelSize: 11
            color: root.makeNew && !root.direct ? Theme.accent : Theme.success
            text: !root.info ? "" :
                  root.direct ? "This folder is an OpenDesigner project; it will be opened."
                : root.makeNew ? "A new, empty workspace is set up in opendesigner/workspaces/ here. The agent decides what to make, and can start more workspaces for other deliverables."
                : "The workspace is opened as you left it."
        }

        Label {
            visible: root.recent.length > 0
            text: "Recent"
            color: Theme.muted
            font.pixelSize: 11
            font.bold: true
        }
        Flow {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: root.recent
                delegate: Btn {
                    required property var modelData
                    text: modelData.name
                    onClicked: root.choose("file://" + modelData.folder)
                    ToolTip.visible: hovered
                    ToolTip.text: modelData.folder
                }
            }
        }

        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

        Label { text: "Your agent"; color: Theme.text; font.pixelSize: 15; font.bold: true }
        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: agentBox
                Layout.preferredWidth: 190
                model: root.presets.map(p => p.found ? p.label + "  (found)" : p.label)
            }
            Field {
                id: customCommand
                Layout.fillWidth: true
                visible: root.presets[agentBox.currentIndex].command === ""
                placeholderText: "command that starts your agent"
            }
            Label {
                Layout.fillWidth: true
                visible: root.presets[agentBox.currentIndex].command !== ""
                text: root.presets[agentBox.currentIndex].command
                color: Theme.muted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: "You stay signed in to the agent yourself. It is started on this computer when you press the button, and its work appears here live."
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: "It also gets the MCP servers you added in the Connect panel, so it can call them (speech models, for example) while it works."
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.agentChosen
            spacing: 8
            Label { text: "Model"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                Layout.preferredWidth: 220
                enabled: !!root.modelOption
                model: ["Agent default"].concat(root.modelOption ? root.modelOption.choices.map(c => c.name) : [])
                currentIndex: {
                    if (!root.modelOption || root.modelValue === "") return 0
                    const i = root.modelOption.choices.findIndex(c => c.value === root.modelValue)
                    return i >= 0 ? i + 1 : 0
                }
                onActivated: i => root.modelValue = i === 0 ? "" : root.modelOption.choices[i - 1].value
            }
            Label { text: "Effort"; color: Theme.muted; font.pixelSize: 12 }
            ComboBox {
                Layout.preferredWidth: 130
                enabled: !!root.effortOption
                model: ["Agent default"].concat(root.effortOption ? root.effortOption.choices.map(c => c.name) : [])
                currentIndex: {
                    if (!root.effortOption || root.effortValue === "") return 0
                    const i = root.effortOption.choices.findIndex(c => c.value === root.effortValue)
                    return i >= 0 ? i + 1 : 0
                }
                onActivated: i => root.effortValue = i === 0 ? "" : root.effortOption.choices[i - 1].value
            }
            BusyIndicator { running: visible; visible: root.probeMatches && !!root.probe.loading; implicitWidth: 18; implicitHeight: 18 }
            Btn { text: "Refresh"; implicitHeight: 24; onClicked: root.readOptions() }
        }
        Label {
            Layout.fillWidth: true
            visible: root.agentChosen
            wrapMode: Text.WordWrap
            color: root.probeMatches && root.probe.error ? Theme.danger : Theme.muted
            font.pixelSize: 11
            text: !root.probeMatches || root.probe.loading ? "Reading the models this agent offers (it is started briefly)…"
                : root.probe.error ? "Could not read the agent's models: " + root.probe.error
                : !root.modelOption && !root.effortOption ? "This agent does not offer a choice of model or effort here; you can still use its own commands in the chat."
                : "Chosen here, applied when the agent starts. You can change them in the chat any time."
        }

        Label { text: "Size and format (optional)"; color: Theme.text; font.pixelSize: 15; font.bold: true }
        RowLayout {
            Layout.fillWidth: true
            Label { Layout.fillWidth: true; text: root.formatLabel; color: root.format ? Theme.text : Theme.muted; elide: Text.ElideRight }
            Btn { text: formatPicker.visible ? "Done" : "Choose…"; onClicked: formatPicker.visible = !formatPicker.visible }
        }
        FormatPicker {
            id: formatPicker
            visible: false
            Layout.fillWidth: true
            editor: root.editor
            current: root.format ? (root.format.id || "custom") : ""
            onPicked: f => { root.format = f; formatPicker.visible = false }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: "If you pick nothing the agent chooses what fits (a reel, a store set, a landing page). If you do pick, it keeps to it and asks you before changing it."
        }

        RowLayout {
            Layout.fillWidth: true
            Label { Layout.fillWidth: true; text: "What should it make first?"; color: Theme.text; font.pixelSize: 15; font.bold: true }
            Btn {
                text: "Direction…"
                onClicked: direction.open()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Pick a look, colors, type and mood; they go into the prompt"
            }
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 96
            TextArea {
                id: prompt
                wrapMode: TextEdit.Wrap
                color: Theme.text
                font.pixelSize: Theme.fontSize
                placeholderText: "For example: start the app, take screenshots of its main screens and make the App Store set. Or: cut the footage I imported into a 30 second teaser with titles."
                placeholderTextColor: Theme.muted
                enabled: root.agentChosen
                background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: prompt.activeFocus ? Theme.accent : Theme.border }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Btn {
                text: root.agentChosen && root.info ? "Open and start the agent" : "Open"
                primary: true
                enabled: root.info !== null && root.info.exists && (root.direct || !root.makeNew || newName.text.trim() !== "")
                onClicked: {
                    const name = root.direct ? "" : (root.makeNew ? newName.text.trim() : root.existing[workspaceBox.currentIndex])
                    if (!root.editor.openWorkspace(root.folderUrl, name, true)) return
                    if (root.format) root.editor.setFormat(root.format.id || "", root.format.kind || "", root.format.width || 0, root.format.height || 0, root.format.fps || 0)
                    if (root.agentChosen) root.editor.chatStartWith(root.command, prompt.text, root.modelValue, root.effortValue)
                    root.started()
                }
            }
        }
    }
}
