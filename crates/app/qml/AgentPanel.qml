import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The loop with an agent: the notes you pinned, what the agent did (with a restore point before
// each of its turns), and how to connect an agent.
Rectangle {
    id: root
    required property var editor
    signal toast(string text)
    color: Theme.panel

    readonly property var notes: editor.annotations ? JSON.parse(editor.annotations) : []
    readonly property var log: editor.log ? JSON.parse(editor.log) : []
    readonly property var checkpoints: editor.checkpoints ? JSON.parse(editor.checkpoints) : []
    property var skillRows: []
    property string skillResult: ""
    property alias tab: bar.currentIndex

    function refreshSkill() { skillRows = JSON.parse(editor.skillStatus()) }
    Component.onCompleted: refreshSkill()

    function copy(text) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        toast("Copied")
    }
    TextEdit { id: clipboard; visible: false }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TabBar {
            id: bar
            Layout.fillWidth: true
            background: Rectangle { color: Theme.panel }
            Repeater {
                model: ["Notes", "History", "Connect", "Chat"]
                TabButton {
                    required property string modelData
                    required property int index
                    text: index === 0 ? modelData + (root.notes.filter(n => n.status === "open").length > 0 ? " (" + root.notes.filter(n => n.status === "open").length + ")" : "") : modelData
                    font.pixelSize: Theme.fontSize
                    contentItem: Text {
                        text: parent.text
                        font: parent.font
                        color: parent.checked ? Theme.accent : Theme.muted
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle {
                        color: "transparent"
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: parent.parent.checked ? Theme.accent : "transparent" }
                    }
                }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: bar.currentIndex

            // ---- notes ----
            ColumnLayout {
                spacing: 0
                Label {
                    visible: root.notes.length === 0
                    Layout.fillWidth: true
                    Layout.margins: Theme.pad
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: Theme.fontSize
                    text: "Pin a note on a layer (the pin tool, or select a layer and use the inspector). The agent reads your open notes before it acts, and resolves them when it is done."
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: root.notes
                    spacing: 6
                    topMargin: 8
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        id: card
                        required property var modelData
                        x: 8
                        width: ListView.view.width - 16
                        height: col.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.panelRaised
                        opacity: modelData.status === "open" ? 1 : 0.55
                        border.width: 1
                        border.color: modelData.status === "open" ? Qt.rgba(1, 0.62, 0.26, 0.5) : Theme.border
                        ColumnLayout {
                            id: col
                            anchors.fill: parent
                            anchors.margins: 8
                            spacing: 4
                            RowLayout {
                                Rectangle {
                                    Layout.preferredWidth: 18; Layout.preferredHeight: 18; radius: 9
                                    color: card.modelData.status === "open" ? Theme.pin : Theme.success
                                    Text { anchors.centerIn: parent; text: card.modelData.id; color: "#2a1500"; font.pixelSize: 10; font.bold: true }
                                }
                                Label {
                                    text: card.modelData.author === "user" ? "you" : card.modelData.author.replace("agent:", "")
                                    color: Theme.muted
                                    font.pixelSize: 11
                                    Layout.fillWidth: true
                                }
                                Label {
                                    visible: !card.modelData.target_exists
                                    text: "target deleted"
                                    color: Theme.danger
                                    font.pixelSize: 11
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: card.modelData.note
                                wrapMode: Text.WordWrap
                                color: Theme.text
                                font.pixelSize: Theme.fontSize
                            }
                            Label {
                                visible: !!card.modelData.resolution
                                Layout.fillWidth: true
                                text: "✓ " + (card.modelData.resolved_by || "").replace("agent:", "") + ": " + card.modelData.resolution
                                wrapMode: Text.WordWrap
                                color: Theme.success
                                font.pixelSize: 12
                            }
                            RowLayout {
                                spacing: 4
                                Btn {
                                    visible: card.modelData.status === "open"
                                    text: "Resolve"
                                    implicitHeight: 24
                                    onClicked: root.editor.resolvePin(card.modelData.id)
                                }
                                Btn {
                                    visible: card.modelData.status !== "open"
                                    text: "Reopen"
                                    implicitHeight: 24
                                    onClicked: root.editor.reopenPin(card.modelData.id)
                                }
                                Btn {
                                    text: "Delete"
                                    implicitHeight: 24
                                    tint: Theme.danger
                                    onClicked: root.editor.removePin(card.modelData.id)
                                }
                            }
                        }
                    }
                }
            }

            // ---- history ----
            ColumnLayout {
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 8
                    spacing: 6
                    Btn {
                        text: "Save checkpoint"
                        onClicked: root.editor.checkpoint("manual " + new Date().toLocaleTimeString(Qt.locale(), "HH:mm:ss"))
                    }
                    Btn {
                        text: "Clear outlines"
                        enabled: root.editor.changed !== "[]"
                        onClicked: root.editor.clearAgentChanges()
                    }
                }
                Label {
                    visible: root.checkpoints.length > 0
                    text: "Restore points"
                    color: Theme.muted
                    font.pixelSize: 11
                    font.bold: true
                    Layout.leftMargin: 10
                }
                Repeater {
                    model: root.checkpoints.slice().reverse().slice(0, 6)
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        Layout.rightMargin: 10
                        Label {
                            text: modelData.name
                            color: Theme.text
                            elide: Text.ElideRight
                            font.pixelSize: 12
                            Layout.fillWidth: true
                        }
                        Label { text: modelData.actor.replace("agent:", ""); color: Theme.muted; font.pixelSize: 11 }
                        Btn { text: "Restore"; implicitHeight: 22; onClicked: root.editor.restore(modelData.id) }
                    }
                }
                Label {
                    text: "Activity"
                    color: Theme.muted
                    font.pixelSize: 11
                    font.bold: true
                    Layout.leftMargin: 10
                    Layout.topMargin: 8
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: root.log
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        required property var modelData
                        width: ListView.view.width
                        height: 34
                        color: "transparent"
                        Rectangle { width: 3; height: parent.height - 8; y: 4; color: modelData.agent ? Theme.agent : Theme.accentStrong }
                        ColumnLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            spacing: 0
                            Label {
                                text: (modelData.agent ? modelData.actor.replace("agent:", "") : "you") + (modelData.kind.type !== "edit" ? " · " + modelData.kind.type : "")
                                color: modelData.agent ? Theme.agent : Theme.text
                                font.pixelSize: 12
                                font.bold: true
                            }
                            Label { text: modelData.summary; color: Theme.muted; font.pixelSize: 11; elide: Text.ElideRight; Layout.fillWidth: true }
                        }
                    }
                }
            }

            // ---- connect ----
            ScrollView {
                id: connectScroll
                contentWidth: availableWidth
                clip: true
                ColumnLayout {
                    width: connectScroll.availableWidth - 2 * Theme.pad
                    x: Theme.pad
                    y: Theme.pad
                    spacing: 8

                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: Theme.fontSize
                        text: "Let a coding agent (Claude Code, Codex, …) work in this project. It connects on this computer only, with a token only you can read, and everything it does is outlined, checkpointed and reversible."
                    }
                    RowLayout {
                        Btn {
                            primary: !root.editor.mcpRunning
                            text: root.editor.mcpRunning ? "Stop agent connection" : "Start agent connection"
                            onClicked: root.editor.mcpRunning ? root.editor.stopMcp() : root.editor.startMcp()
                        }
                    }
                    Label {
                        visible: root.editor.mcpRunning
                        text: "Listening on " + root.editor.mcpUrl
                        color: Theme.success
                        font.pixelSize: 12
                    }
                    ColumnLayout {
                        visible: root.editor.mcpRunning
                        Layout.fillWidth: true
                        spacing: 4
                        Label { text: "Connect Claude Code"; color: Theme.text; font.pixelSize: 12; font.bold: true }
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: cmd.implicitHeight + 12
                            radius: 4
                            color: Theme.field
                            border.width: 1
                            border.color: Theme.border
                            Text {
                                id: cmd
                                anchors.fill: parent
                                anchors.margins: 6
                                text: root.editor.connectCommand()
                                color: Theme.text
                                font.family: "Menlo"
                                font.pixelSize: 11
                                wrapMode: Text.WrapAnywhere
                            }
                        }
                        Btn { text: "Copy command"; onClicked: root.copy(root.editor.connectCommand()) }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            color: Theme.muted
                            font.pixelSize: 11
                            text: "Other agents: this address with the header Authorization: Bearer <the token in the file the command reads>."
                        }
                    }

                    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

                    Label { text: "Teach your agents"; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: 12
                        text: "Installs a skill (a short guide and the store rules) where your agents look for skills. Only a folder named opendesigner is written; nothing else is touched, and you can remove it again."
                    }
                    Repeater {
                        model: root.skillRows
                        delegate: ColumnLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 2
                            Label {
                                text: modelData.label + " — " + modelData.readers.join(", ")
                                color: Theme.text
                                font.pixelSize: 12
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                            }
                            Label {
                                text: (modelData.detected ? "found · " : "not found · ") + modelData.state + (modelData.version ? " " + modelData.version : "")
                                color: modelData.detected ? Theme.success : Theme.muted
                                font.pixelSize: 11
                            }
                            Label {
                                text: modelData.folder
                                color: Theme.muted
                                font.pixelSize: 11
                                elide: Text.ElideMiddle
                                Layout.fillWidth: true
                            }
                        }
                    }
                    RowLayout {
                        Btn {
                            text: "Install the skill…"
                            primary: true
                            onClicked: skillConfirm.open()
                        }
                        Btn { text: "Refresh"; onClicked: root.refreshSkill() }
                    }
                    Label {
                        visible: root.skillResult !== ""
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.success
                        font.pixelSize: 12
                        text: root.skillResult
                    }
                }
            }

            // ---- chat ----
            ChatPanel { editor: root.editor }
        }
    }

    Dialog {
        id: skillConfirm
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, Overlay.overlay ? Overlay.overlay.width - 40 : 560)
        title: "Install the OpenDesigner skill?"
        standardButtons: Dialog.Ok | Dialog.Cancel
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: skillConfirm.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "These files will be written for the agents found on this computer:"
            }
            Repeater {
                model: root.skillRows.filter(r => r.detected)
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Repeater {
                        model: modelData.files
                        Label { required property string modelData; text: modelData; color: Theme.text; font.pixelSize: 11; elide: Text.ElideMiddle; Layout.fillWidth: true }
                    }
                }
            }
            Label { visible: root.skillRows.filter(r => r.detected).length === 0; text: "No supported agent was found."; color: Theme.danger }
        }
        onAccepted: {
            const r = JSON.parse(root.editor.installSkill())
            root.skillResult = r.error ? "" : "Installed for " + r.map(x => x.target).join(", ") + "."
            root.refreshSkill()
        }
    }
}
