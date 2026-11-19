import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A conversation with the coding agent the user already uses (over the Agent Client Protocol).
// The agent changes the design through the editor's agent connection, so its edits are outlined and
// can be restored like any other agent's.
Item {
    id: root
    required property var editor

    readonly property var chat: editor.chat ? JSON.parse(editor.chat) : ({ status: "off", messages: [], permission: null, plan: [] })
    JsonModel { id: messagesModel }
    JsonModel { id: planModel }
    JsonModel { id: optionsModel }
    onChatChanged: {
        messagesModel.sync(chat.messages)
        planModel.sync(chat.plan || [])
        optionsModel.sync(chat.permission ? chat.permission.options : [])
        Qt.callLater(() => list.positionViewAtEnd())
    }
    readonly property bool connected: chat.status === "ready" || chat.status === "busy"
    readonly property var presets: JSON.parse(editor.chatPresets())
    // the agent's commands, filtered by what is typed
    readonly property var allCommands: root.chat.commands || []
    property bool showCommands: false
    readonly property var commandList: {
        if (!root.showCommands) return []
        const typed = inputText.text.trim()
        const q = typed.startsWith("/") ? typed.slice(1).split(/\s/)[0].toLowerCase() : ""
        return root.allCommands.filter(c => q === "" || c.name.toLowerCase().indexOf(q) === 0).slice(0, 40)
    }
    function updateCommands() {
        const typed = inputText.text
        root.showCommands = typed.startsWith("/") && typed.indexOf(" ") < 0 && root.allCommands.length > 0
    }
    function useCommand(name) {
        inputText.text = "/" + name + " "
        inputText.forceActiveFocus()
        root.showCommands = false
    }
    function send() {
        const text = inputText.text.trim()
        if (text === "") return
        root.editor.chatSend(text)
        inputText.text = ""
        root.showCommands = false
    }
    function openLink(link) { Qt.openUrlExternally(link) }
    function focusInput() { if (connected) inputText.forceActiveFocus() }

    DirectionPicker {
        id: direction
        editor: root.editor
        onPicked: text => {
            if (text === "") return
            const current = inputText.text.trim()
            inputText.text = current === "" ? text : current + "\n\n" + text
            inputText.forceActiveFocus()
        }
    }

    Component.onCompleted: {
        const last = editor.chatLastCommand()
        if (last !== "") commandField.text = last
        const i = presets.findIndex(p => p.command === last)
        const firstFound = Math.max(0, presets.findIndex(p => p.found))
        presetBox.currentIndex = i >= 0 ? i : (last !== "" ? presets.length - 1 : firstFound)
        if (last === "") commandField.text = presets[firstFound].command
    }


    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.pad
        spacing: 8

        // connection
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            visible: !root.connected
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "Chat with the coding agent you already use. OpenDesigner starts it on your machine, in this project's folder, and gives it the editor's tools; you stay signed in to it yourself. Agents found on this computer are marked. Sign in to the agent first; the first start of Claude Code or Codex downloads an adapter and can take a minute or two."
            }
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: presetBox
                    model: root.presets.map(p => p.found ? p.label + "  (found)" : p.label)
                    Layout.preferredWidth: 170
                    onActivated: i => { if (root.presets[i].command !== "") commandField.text = root.presets[i].command }
                }
                Field { id: commandField; Layout.fillWidth: true; placeholderText: "command that starts your agent" }
            }
            RowLayout {
                Layout.fillWidth: true
                Btn {
                    text: root.chat.status === "starting" ? "Starting the agent…" : "Connect"
                    primary: true
                    enabled: root.chat.status !== "starting" && commandField.text.trim() !== "" && root.editor.hasProject
                    onClicked: root.editor.chatStart(commandField.text)
                }
                Label { Layout.fillWidth: true; color: Theme.muted; font.pixelSize: 11; text: "This also turns on the agent connection." }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.connected
            Label {
                Layout.fillWidth: true
                color: root.chat.status === "busy" ? Theme.accent : Theme.muted
                font.pixelSize: 12
                text: root.chat.status === "busy" ? "The agent is working…" : "Connected"
                leftPadding: busy.visible ? 24 : 0
                BusyIndicator { id: busy; running: visible; visible: root.chat.status === "busy"; implicitWidth: 18; implicitHeight: 18; anchors.verticalCenter: parent.verticalCenter }
            }
            Btn { text: "Cancel"; visible: root.chat.status === "busy"; onClicked: root.editor.chatCancel() }
            CheckBox {
                id: allowBox
                checked: root.chat.auto_allow === true
                text: "Always allow"
                font.pixelSize: 11
                onToggled: root.editor.chatAutoAllow(checked)
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Answer the agent's permission questions with Allow, without asking each time"
                contentItem: Text { text: parent.text; font: parent.font; color: Theme.muted; leftPadding: parent.indicator.width + 4; verticalAlignment: Text.AlignVCenter }
            }
            Btn { text: "Disconnect"; onClicked: root.editor.chatStop() }
        }

        // what the agent is set to: model, mode, how hard it thinks
        Flow {
            Layout.fillWidth: true
            visible: root.connected && root.chat.config && root.chat.config.length > 0
            spacing: 6
            Repeater {
                model: root.chat.config || []
                delegate: RowLayout {
                    required property var modelData
                    spacing: 4
                    Label {
                        color: Theme.muted
                        font.pixelSize: 11
                        text: modelData.category === "thought_level" ? "Effort"
                            : modelData.category === "model" ? "Model"
                            : modelData.category === "mode" ? "Mode"
                            : modelData.name
                    }
                    ComboBox {
                        id: settingBox
                        required property var modelData
                        implicitWidth: Math.min(230, Math.max(110, settingBox.currentText.length * 7 + 40))
                        implicitHeight: Theme.row
                        font.pixelSize: 11
                        model: settingBox.modelData.choices.map(c => c.name + (c.description ? "  ·  " + c.description : ""))
                        currentIndex: {
                            const at = settingBox.modelData.choices.findIndex(c => c.value === settingBox.modelData.current)
                            return at >= 0 ? at : 0
                        }
                        onActivated: i => root.editor.chatSetConfig(settingBox.modelData.id, settingBox.modelData.choices[i].value)
                        ToolTip.visible: hovered
                        ToolTip.text: settingBox.modelData.choices.length + " choices"
                    }
                }
            }
        }

        // the agent's plan, as it stands: done, doing, to do
        Rectangle {
            Layout.fillWidth: true
            visible: root.chat.plan && root.chat.plan.length > 0
            implicitHeight: planColumn.implicitHeight + 12
            radius: Theme.radius
            color: Theme.panelRaised
            border.width: 1
            border.color: Theme.border
            ColumnLayout {
                id: planColumn
                anchors.fill: parent
                anchors.margins: 6
                spacing: 2
                Label { text: "Plan"; color: Theme.text; font.pixelSize: 11; font.bold: true }
                Repeater {
                    model: planModel
                    delegate: Label {
                        required property string json
                        readonly property var modelData: JSON.parse(json)
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font.pixelSize: 11
                        color: modelData.status === "in_progress" ? Theme.accent : Theme.muted
                        font.bold: modelData.status === "in_progress"
                        text: (modelData.status === "completed" ? "✓ " : modelData.status === "in_progress" ? "▸ " : "○ ") + modelData.content
                    }
                }
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 6
            model: messagesModel
            delegate: Rectangle {
                id: bubble
                required property string json
                readonly property var modelData: JSON.parse(json)
                width: ListView.view.width
                implicitHeight: body.implicitHeight + 12
                radius: Theme.radius
                color: modelData.role === "user" ? Theme.panelRaised
                     : modelData.role === "error" ? Qt.rgba(0.8, 0.2, 0.2, 0.18)
                     : "transparent"
                border.width: modelData.role === "tool" ? 1 : 0
                border.color: Theme.border
                // a text edit, so the user can select and copy what the agent said
                TextEdit {
                    id: body
                    anchors.fill: parent
                    anchors.margins: 6
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    font.pixelSize: modelData.role === "thought" || modelData.role === "tool" ? 11 : Theme.fontSize
                    font.italic: modelData.role === "thought"
                    color: modelData.role === "agent" || modelData.role === "user" ? Theme.text
                         : modelData.role === "error" ? "#ff9a9a" : Theme.muted
                    selectionColor: Theme.accentStrong
                    selectedTextColor: Theme.textOnAccent
                    text: modelData.queued ? "You (queued): " + modelData.text : modelData.role === "tool" ? (modelData.status === "completed" ? "✓ " : modelData.status === "failed" ? "✗ " : "… ") + modelData.title
                        : modelData.role === "user" ? "You: " + modelData.text
                        : modelData.text
                    onLinkActivated: link => root.openLink(link)
                }
            }
        }

        // the agent asks before it does something the user should decide
        Rectangle {
            Layout.fillWidth: true
            visible: !!root.chat.permission
            implicitHeight: permissionColumn.implicitHeight + 16
            radius: Theme.radius
            color: Theme.panelRaised
            border.width: 1
            border.color: Theme.accent
            ColumnLayout {
                id: permissionColumn
                anchors.fill: parent
                anchors.margins: 8
                spacing: 6
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    text: root.chat.permission ? "The agent asks: " + root.chat.permission.title : ""
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: optionsModel
                        delegate: Btn {
                            required property string json
                            readonly property var modelData: JSON.parse(json)
                            text: modelData.name
                            primary: modelData.kind === "allow_once"
                            onClicked: root.editor.chatAnswer(root.chat.permission.request, modelData.id)
                        }
                    }
                    Btn { text: "Cancel"; onClicked: root.editor.chatAnswer(root.chat.permission.request, "") }
                }
            }
        }

        // the slash commands the agent understands
        Popup {
            id: commandPopup
            width: Math.min(420, parent ? parent.width - 20 : 420)
            y: -commandList.implicitHeight - 8
            padding: 6
            visible: root.commandList.length > 0 && root.showCommands
            background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
            contentItem: ListView {
                id: commandList
                implicitHeight: Math.min(260, contentHeight)
                model: root.commandList
                clip: true
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property var modelData
                    width: commandList.width
                    height: 44
                    color: "transparent"
                    Label {
                        anchors.fill: parent
                        anchors.margins: 4
                        text: "/" + modelData.name + (modelData.hint ? " " + modelData.hint : "") + "\n" + modelData.description
                        color: Theme.text
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                        elide: Text.ElideRight
                    }
                    TapHandler { onTapped: root.useCommand(modelData.name) }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.connected
            spacing: 6
            Btn {
                text: "Direction"
                onClicked: direction.open()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Pick a look, colors, type and mood; they go into the prompt"
            }
            Btn {
                text: "/"
                onClicked: { root.showCommands = true; inputText.forceActiveFocus() }
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "The agent's own commands"
                enabled: (root.chat.commands || []).length > 0
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(110, Math.max(34, inputText.implicitHeight + 8))
                clip: true
                TextArea {
                    id: inputText
                    width: Math.max(0, parent ? parent.width : 0)
                    wrapMode: TextEdit.Wrap
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    placeholderText: root.chat.status === "busy" ? "Type now; it is sent when the agent is done" : "Ask for a change (your selection and notes are sent along)"
                    placeholderTextColor: Theme.muted
                    enabled: root.chat.status === "ready" || root.chat.status === "busy"
                    background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: inputText.activeFocus ? Theme.accent : Theme.border }
                    onTextChanged: root.updateCommands()
                    Keys.onPressed: key => {
                        if (key.key === Qt.Key_Return && !(key.modifiers & Qt.ShiftModifier)) {
                            root.send()
                            key.accepted = true
                        } else if (key.key === Qt.Key_Tab && root.showCommands && root.commandList.length > 0) {
                            root.useCommand(root.commandList[0].name)
                            key.accepted = true
                        }
                    }
                }
            }
            Btn {
                text: "Send"
                primary: true
                enabled: (root.chat.status === "ready" || root.chat.status === "busy") && inputText.text.trim() !== ""
                onClicked: root.send()
            }
        }
    }
}
