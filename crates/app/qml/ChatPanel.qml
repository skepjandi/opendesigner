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
    // the user closed the list by hand: it stays closed until the text stops being a command
    property bool commandsDismissed: false
    property int commandIndex: 0
    readonly property var commandList: {
        if (!root.showCommands) return []
        const typed = inputText.text.trim()
        const q = typed.startsWith("/") ? typed.slice(1).split(/\s/)[0].toLowerCase() : ""
        return root.allCommands.filter(c => q === "" || c.name.toLowerCase().indexOf(q) === 0).slice(0, 40)
    }
    onCommandListChanged: commandIndex = 0
    function updateCommands() {
        const typed = inputText.text
        if (!typed.startsWith("/")) root.commandsDismissed = false
        root.showCommands = !root.commandsDismissed && typed.startsWith("/") && typed.indexOf(" ") < 0 && root.allCommands.length > 0
    }
    function dismissCommands() {
        root.commandsDismissed = true
        root.showCommands = false
    }
    // commands that are really settings of the session open a chooser here instead of being
    // sent to the model as text
    function categoryOf(cmd) {
        switch (cmd) {
        case "model": case "switch": case "modelpreset": return "model"
        case "effort": case "thinking": case "think": case "reasoning": return "thought_level"
        case "mode": case "plan": return "mode"
        default: return ""
        }
    }
    function optionFor(cmd) {
        const cat = categoryOf(cmd)
        return cat === "" ? null : ((root.chat.config || []).find(o => o.category === cat) || null)
    }
    function openChoice(opt) {
        root.choiceOption = opt
        choiceDialog.open()
    }
    property var choiceOption: null
    function useCommand(name) {
        root.showCommands = false
        const opt = optionFor(name)
        if (opt) { inputText.text = ""; openChoice(opt); return }
        inputText.text = "/" + name + " "
        inputText.forceActiveFocus()
    }
    function send() {
        const text = inputText.text.trim()
        if (text === "") return
        if (text.startsWith("/") && text.indexOf(" ") < 0) {
            const opt = optionFor(text.slice(1).toLowerCase())
            if (opt) { inputText.text = ""; root.showCommands = false; openChoice(opt); return }
        }
        root.editor.chatSend(text)
        inputText.text = ""
        root.showCommands = false
    }
    function openLink(link) { Qt.openUrlExternally(link) }
    function focusInput() { if (connected) inputText.forceActiveFocus() }

    Dialog {
        id: choiceDialog
        modal: true
        parent: Overlay.overlay
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, Overlay.overlay ? Overlay.overlay.width - 60 : 460)
        height: Math.min(520, Overlay.overlay ? Overlay.overlay.height - 60 : 520)
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        title: root.choiceOption ? root.choiceOption.name : ""
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: choiceDialog.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }
        contentItem: ListView {
            id: choiceList
            clip: true
            model: root.choiceOption ? root.choiceOption.choices : []
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                id: choiceRow
                required property var modelData
                readonly property bool current: root.choiceOption && modelData.value === root.choiceOption.current
                width: choiceList.width
                height: choiceText.implicitHeight + 14
                radius: 4
                color: choiceHover.hovered ? Theme.field : "transparent"
                border.width: current ? 1 : 0
                border.color: Theme.accent
                Label {
                    id: choiceText
                    x: 10; y: 7
                    width: parent.width - 20
                    text: choiceRow.modelData.name + (choiceRow.modelData.description && choiceRow.modelData.description !== choiceRow.modelData.name ? "\n" + choiceRow.modelData.description : "")
                    color: choiceRow.current ? Theme.accent : Theme.text
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                }
                HoverHandler { id: choiceHover }
                TapHandler {
                    onTapped: {
                        root.editor.chatSetConfig(root.choiceOption.id, choiceRow.modelData.value)
                        choiceDialog.close()
                    }
                }
            }
        }
        footer: RowLayout {
            Item { Layout.fillWidth: true }
            Btn { text: "Close"; onClicked: choiceDialog.close() }
            Item { implicitWidth: 6 }
        }
    }

    // the session's settings and the agent's commands, from one button
    Menu {
        id: optionsMenu
        Instantiator {
            model: root.chat.config || []
            delegate: MenuItem {
                required property var modelData
                text: (modelData.category === "thought_level" ? "Effort" : modelData.category === "model" ? "Model" : modelData.category === "mode" ? "Mode" : modelData.name) + ":  " + (modelData.choices.find(c => c.value === modelData.current) || { name: modelData.current }).name
                onTriggered: root.openChoice(modelData)
            }
            onObjectAdded: (i, o) => optionsMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => optionsMenu.removeItem(o)
        }
        MenuSeparator { visible: (root.chat.commands || []).length > 0 }
        MenuItem {
            visible: (root.chat.commands || []).length > 0
            height: visible ? implicitHeight : 0
            text: "The agent's commands…"
            onTriggered: { root.commandsDismissed = false; root.showCommands = true; inputText.forceActiveFocus() }
        }
    }

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
                    id: settingRow
                    required property var modelData
                    spacing: 4
                    Label {
                        color: Theme.muted
                        font.pixelSize: 11
                        text: settingRow.modelData.category === "thought_level" ? "Effort"
                            : settingRow.modelData.category === "model" ? "Model"
                            : settingRow.modelData.category === "mode" ? "Mode"
                            : settingRow.modelData.name
                    }
                    ComboBox {
                        id: settingBox
                        implicitWidth: Math.min(230, Math.max(110, settingBox.currentText.length * 7 + 40))
                        implicitHeight: Theme.row
                        font.pixelSize: 11
                        model: settingRow.modelData.choices.map(c => c.name + (c.description ? "  ·  " + c.description : ""))
                        currentIndex: {
                            const at = settingRow.modelData.choices.findIndex(c => c.value === settingRow.modelData.current)
                            return at >= 0 ? at : 0
                        }
                        onActivated: i => root.editor.chatSetConfig(settingRow.modelData.id, settingRow.modelData.choices[i].value)
                        ToolTip.visible: hovered
                        ToolTip.text: settingRow.modelData.choices.length + " choices"
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
                    delegate: RowLayout {
                        required property string json
                        readonly property var modelData: JSON.parse(json)
                        Layout.fillWidth: true
                        spacing: 4
                        Icon {
                            Layout.alignment: Qt.AlignTop
                            name: modelData.status === "completed" ? "check" : modelData.status === "in_progress" ? "chevron-right" : "circle"
                            size: 12
                            color: modelData.status === "in_progress" ? Theme.accent : Theme.muted
                        }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font.pixelSize: 11
                            color: modelData.status === "in_progress" ? Theme.accent : Theme.muted
                            font.bold: modelData.status === "in_progress"
                            text: modelData.content
                        }
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
                    leftPadding: modelData.role === "tool" ? 18 : 0
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
                    text: modelData.queued ? "You (queued): " + modelData.text : modelData.role === "tool" ? modelData.title
                        : modelData.role === "user" ? "You: " + modelData.text
                        : modelData.text
                    onLinkActivated: link => root.openLink(link)
                }
                Icon {
                    visible: modelData.role === "tool"
                    x: 6
                    y: 8
                    name: modelData.status === "completed" ? "check" : modelData.status === "failed" ? "x" : "ellipsis"
                    size: 12
                    color: modelData.status === "failed" ? Theme.danger : Theme.muted
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

        RowLayout {
            id: inputRow
            Layout.fillWidth: true
            visible: root.connected
            spacing: 6

            // the agent's slash commands, listed just above the message field
            Popup {
                id: commandPopup
                // wanted while the user is typing a command; closing it by hand (Esc, the close
                // button, a click elsewhere) holds it shut until the text is no longer a command
                readonly property bool wanted: root.showCommands && root.commandList.length > 0
                onWantedChanged: { if (wanted) open(); else close() }
                onClosed: { if (wanted) root.dismissCommands() }
                Component.onCompleted: { if (wanted) open() }
                parent: inputRow
                x: 0
                width: Math.min(460, inputRow.width)
                height: Math.min(340, commandList.contentHeight + 52)
                y: -height - 6
                padding: 6
                focus: false
                closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
                contentItem: ColumnLayout {
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            Layout.fillWidth: true
                            leftPadding: 4
                            text: "The agent's commands  ·  ↑↓ choose, Tab or Enter fills in, Esc closes"
                            color: Theme.muted
                            font.pixelSize: 10
                            elide: Text.ElideRight
                        }
                        Btn {
                            iconName: "x"
                            Accessible.name: "Close the command list"
                            tip: "Close (Esc)"
                            implicitWidth: 24
                            implicitHeight: 22
                            leftPadding: 0
                            rightPadding: 0
                            onClicked: root.dismissCommands()
                        }
                    }
                    ListView {
                        id: commandList
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        model: root.commandList
                        currentIndex: root.commandIndex
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar {}
                        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                        delegate: Rectangle {
                            id: cmdRow
                            required property var modelData
                            required property int index
                            width: commandList.width
                            height: cmdText.implicitHeight + 10
                            radius: 4
                            color: cmdRow.index === root.commandIndex || cmdTap.hovered ? Theme.field : "transparent"
                            Label {
                                id: cmdText
                                x: 6; y: 5
                                width: parent.width - 12
                                text: "/" + cmdRow.modelData.name + (cmdRow.modelData.hint ? " " + cmdRow.modelData.hint : "") + (cmdRow.modelData.description ? "\n" + cmdRow.modelData.description : "")
                                color: Theme.text
                                font.pixelSize: 11
                                wrapMode: Text.WordWrap
                                maximumLineCount: 3
                                elide: Text.ElideRight
                            }
                            HoverHandler { id: cmdTap }
                            TapHandler { onTapped: root.useCommand(cmdRow.modelData.name) }
                        }
                    }
                }
            }

            Btn {
                text: "Direction"
                iconName: "compass"
                onClicked: direction.open()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: "Pick a look, colors, type and mood; they go into the prompt"
            }
            Btn {
                iconName: "settings"
                Accessible.name: "Options"
                implicitWidth: 34
                leftPadding: 0
                rightPadding: 0
                onClicked: optionsMenu.popup(this, 0, -optionsMenu.implicitHeight)
                tip: "Model, effort, mode and the agent's commands"
                enabled: (root.chat.config || []).length > 0 || (root.chat.commands || []).length > 0
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
                    // Escape closes the command list first; the window's own Escape (deselect)
                    // only sees it when no list is open
                    Keys.onShortcutOverride: event => { if (event.key === Qt.Key_Escape && commandPopup.opened) event.accepted = true }
                    Keys.onEscapePressed: event => { if (commandPopup.opened) { root.dismissCommands(); event.accepted = true } }
                    Keys.onPressed: key => {
                        const listed = commandPopup.opened && root.commandList.length > 0
                        if (listed && key.key === Qt.Key_Down) {
                            root.commandIndex = Math.min(root.commandList.length - 1, root.commandIndex + 1)
                            key.accepted = true
                        } else if (listed && key.key === Qt.Key_Up) {
                            root.commandIndex = Math.max(0, root.commandIndex - 1)
                            key.accepted = true
                        } else if (key.key === Qt.Key_Tab && listed) {
                            root.useCommand(root.commandList[root.commandIndex].name)
                            key.accepted = true
                        } else if (key.key === Qt.Key_Return && !(key.modifiers & Qt.ShiftModifier)) {
                            // Enter on a half-typed command fills it in; on a whole one it runs
                            const typed = inputText.text.trim()
                            const chosen = listed ? root.commandList[root.commandIndex] : null
                            if (chosen && typed !== "/" + chosen.name) root.useCommand(chosen.name)
                            else root.send()
                            key.accepted = true
                        }
                    }
                }
            }
            Btn {
                text: "Send"
                iconName: "send"
                primary: true
                enabled: (root.chat.status === "ready" || root.chat.status === "busy") && inputText.text.trim() !== ""
                onClicked: root.send()
            }
        }
    }
}
