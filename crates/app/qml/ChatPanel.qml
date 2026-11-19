import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A conversation with the coding agent the user already uses (over the Agent Client Protocol).
// The agent changes the design through the editor's agent connection, so its edits are outlined and
// can be restored like any other agent's.
Item {
    id: root
    required property var editor

    readonly property var chat: editor.chat ? JSON.parse(editor.chat) : ({ status: "off", messages: [], permission: null })
    readonly property bool connected: chat.status === "ready" || chat.status === "busy"
    readonly property var presets: JSON.parse(editor.chatPresets())

    Component.onCompleted: {
        const last = editor.chatLastCommand()
        if (last !== "") commandField.text = last
        const i = presets.findIndex(p => p.command === last)
        const firstFound = Math.max(0, presets.findIndex(p => p.found))
        presetBox.currentIndex = i >= 0 ? i : (last !== "" ? presets.length - 1 : firstFound)
        if (last === "") commandField.text = presets[firstFound].command
    }

    onChatChanged: Qt.callLater(() => list.positionViewAtEnd())

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
                    text: root.chat.status === "starting" ? "Starting…" : "Connect"
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
            }
            Btn { text: "Cancel"; visible: root.chat.status === "busy"; onClicked: root.editor.chatCancel() }
            Btn { text: "Disconnect"; onClicked: root.editor.chatStop() }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 6
            model: root.chat.messages
            delegate: Rectangle {
                id: bubble
                required property var modelData
                width: ListView.view.width
                implicitHeight: body.implicitHeight + 12
                radius: Theme.radius
                color: modelData.role === "user" ? Theme.panelRaised
                     : modelData.role === "error" ? Qt.rgba(0.8, 0.2, 0.2, 0.18)
                     : "transparent"
                border.width: modelData.role === "tool" ? 1 : 0
                border.color: Theme.border
                Label {
                    id: body
                    anchors.fill: parent
                    anchors.margins: 6
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font.pixelSize: modelData.role === "thought" || modelData.role === "tool" ? 11 : Theme.fontSize
                    font.italic: modelData.role === "thought"
                    color: modelData.role === "agent" || modelData.role === "user" ? Theme.text
                         : modelData.role === "error" ? "#ff9a9a" : Theme.muted
                    text: modelData.role === "tool" ? (modelData.status === "completed" ? "✓ " : modelData.status === "failed" ? "✗ " : "… ") + modelData.title
                        : modelData.role === "user" ? "You: " + modelData.text
                        : modelData.text
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
                        model: root.chat.permission ? root.chat.permission.options : []
                        delegate: Btn {
                            required property var modelData
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
            Layout.fillWidth: true
            visible: root.connected
            spacing: 6
            Field {
                id: input
                Layout.fillWidth: true
                placeholderText: "Ask for a change (your selection and notes are sent along)"
                enabled: root.chat.status === "ready"
                onAccepted: send()
                function send() {
                    if (text.trim() === "") return
                    root.editor.chatSend(text)
                    text = ""
                }
            }
            Btn { text: "Send"; primary: true; enabled: root.chat.status === "ready" && input.text.trim() !== ""; onClicked: input.send() }
        }
    }
}
