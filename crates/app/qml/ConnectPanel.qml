import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Connecting an agent (what to paste into it), the MCP servers the agent may call, the skill that
// teaches it OpenDesigner, and what it may read on this computer.
Rectangle {
    id: root
    required property var editor
    signal toast(string text)
    color: Theme.panel

    property var skillRows: []
    readonly property var recipes: editor.mcpRunning ? JSON.parse(editor.connectRecipes()) : []
    readonly property var recipe: recipes.length > recipeBox.currentIndex ? recipes[recipeBox.currentIndex] : null
    property string skillResult: ""
    property var servers: []
    property string serverError: ""
    // the name of the server being changed, empty when a new one is added
    property string editing: ""
    function refreshSkill() { skillRows = JSON.parse(editor.skillStatus()) }
    function refreshServers() { servers = JSON.parse(editor.mcpServers()) }
    // where a server runs and which variables it is given, by name only
    function serverDetail(server) {
        const bits = []
        if (server.env && server.env.length > 0) bits.push(server.env.join(", "))
        return bits.join(" · ")
    }
    function newServer() {
        root.editing = ""
        root.serverError = ""
        serverDialog.nameField.text = ""
        serverDialog.commandField.text = ""
        serverDialog.envField.text = ""
        serverDialog.open()
    }
    function editServer(server) {
        root.editing = server.name
        root.serverError = ""
        serverDialog.nameField.text = server.name
        serverDialog.commandField.text = server.command
        // the values are never shown again: a name on its own keeps the stored one
        serverDialog.envField.text = (server.env || []).join("\n")
        serverDialog.open()
    }
    Component.onCompleted: { refreshSkill(); refreshServers() }

    function copy(text) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        toast("Copied")
    }
    TextEdit { id: clipboard; visible: false }

        ScrollView {
        anchors.fill: parent
            id: connectScroll
            contentWidth: availableWidth
            // The layout cannot know how tall wrapped text will be at its final width, so
            // leave room below it: the last control must always be reachable.
            contentHeight: connectColumn.implicitHeight + 2 * Theme.pad + 80
            clip: true
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AlwaysOn }
            ColumnLayout {
                id: connectColumn
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
                    Label { text: "Connect your agent"; color: Theme.text; font.pixelSize: 12; font.bold: true }
                    ComboBox {
                        id: recipeBox
                        Layout.fillWidth: true
                        model: root.recipes.map(r => r.label)
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: 11
                        text: root.recipe ? root.recipe.place : ""
                    }
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
                            text: root.recipe ? root.recipe.text : ""
                            textFormat: Text.PlainText
                            color: Theme.text
                            font.family: "Menlo"
                            font.pixelSize: 11
                            wrapMode: Text.WrapAnywhere
                        }
                    }
                    Btn { text: "Copy"; onClicked: root.copy(root.recipe ? root.recipe.text : "") }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: 11
                        text: root.recipe ? "Checked: " + root.recipe.checked + " The token stays in a file only you can read; nothing here contains it." : ""
                    }
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

                CheckBox {
                    id: deviceBox
                    Layout.fillWidth: true
                    checked: root.editor.deviceAccess()
                    text: "Let the agent find pictures and videos on this computer"
                    font.pixelSize: 12
                    onToggled: root.editor.setDeviceAccess(checked)
                    contentItem: Text { text: parent.text; font: parent.font; color: Theme.text; leftPadding: parent.indicator.width + 6; verticalAlignment: Text.AlignVCenter; wrapMode: Text.WordWrap }
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: "It can search your home folder for pictures and videos by name and copy the ones it needs into this project. It cannot change or delete anything, and it cannot read other kinds of files. Turn this off to limit it to what you add yourself."
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

                Label { text: "Other MCP servers"; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 12
                    text: "MCP servers the agent may call besides the editor's own. They are programs on this computer: the agent starts one when it needs it, and it runs as you, with your rights. Only you can add or change them here; no agent can."
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: "minimax-mcp: the MiniMax MCP server gives the agent speech models (text to speech, transcription). Install it the way MiniMax documents (`npx -y minimax-mcp` or `uvx minimax-mcp`, check their README for the current command), then add it here with the name `minimax-mcp`. Put your MINIMAX_API_KEY in the environment variables: it is kept in the app's config folder and given to that server only."
                }
                Repeater {
                    model: root.servers
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: serverColumn.implicitHeight + 12
                        radius: Theme.radius
                        color: Theme.panelRaised
                        border.width: 1
                        border.color: Theme.border
                        ColumnLayout {
                            id: serverColumn
                            anchors.fill: parent
                            anchors.margins: 6
                            spacing: 2
                            Label {
                                Layout.fillWidth: true
                                text: modelData.name
                                color: Theme.text
                                font.pixelSize: 12
                                font.bold: true
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                text: modelData.command
                                color: Theme.muted
                                font.pixelSize: 11
                                font.family: "Menlo"
                                elide: Text.ElideMiddle
                            }
                            Label {
                                Layout.fillWidth: true
                                visible: root.serverDetail(modelData) !== ""
                                text: root.serverDetail(modelData)
                                color: Theme.muted
                                font.pixelSize: 11
                                elide: Text.ElideMiddle
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Btn { text: "Edit…"; onClicked: root.editServer(modelData) }
                                Btn { text: "Remove"; onClicked: { removeDialog.serverName = modelData.name; removeDialog.open() } }
                                Item { Layout.fillWidth: true }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Btn { text: "Add a server…"; primary: true; onClicked: root.newServer() }
                    Label {
                        Layout.fillWidth: true
                        color: Theme.muted
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                        text: "The agent is given these when a chat starts."
                    }
                }
                Label {
                    visible: root.serverError !== ""
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.danger
                    font.pixelSize: 11
                    text: root.serverError
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

                Label { text: "Sounds from Freesound"; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: "Let the agent find music and sound effects on Freesound (freesound.org) for your videos. A token is free: ask for one at https://freesound.org/apiv2/apply/ and paste it below. It stays in this computer's config folder, and the only place it is sent is freesound.org."
                }
                RowLayout {
                    Layout.fillWidth: true
                    Field {
                        id: freesoundToken
                        Layout.fillWidth: true
                        placeholderText: "Paste your Freesound token"
                        echoMode: TextInput.Password
                    }
                    Btn {
                        text: "Save"
                        onClicked: {
                            root.editor.setFreesoundToken(freesoundToken.text)
                            freesoundToken.text = ""
                        }
                    }
                }
                RowLayout {
                    visible: root.editor.freesoundTokenSet()
                    Layout.fillWidth: true
                    Label {
                        Layout.fillWidth: true
                        text: "A Freesound token is saved. The agent can find sounds; you can remove the token at any time."
                        color: Theme.success
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                    }
                    Btn { text: "Remove"; onClicked: root.editor.clearFreesoundToken() }
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: "Sounds come with their author's license. Whoever publishes the video must show the attribution the agent is given (title, author, license, link)."
                }


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

    Dialog {
        id: skillConfirm
        modal: true
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
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

    Dialog {
        id: serverDialog
        modal: true
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        anchors.centerIn: Overlay.overlay
        width: Math.min(560, Overlay.overlay ? Overlay.overlay.width - 40 : 560)
        title: root.editing === "" ? "Add an MCP server" : "Change the MCP server"
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: serverDialog.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }
        contentItem: ColumnLayout {
            spacing: 6
            Label { text: "Name"; color: Theme.muted; font.pixelSize: 11 }
            Field {
                id: nameField
                Layout.fillWidth: true
                placeholderText: "for example minimax-mcp"
            }
            Label { text: "Command"; color: Theme.muted; font.pixelSize: 11 }
            Field {
                id: commandField
                Layout.fillWidth: true
                placeholderText: "for example npx -y minimax-mcp"
            }
            Label { text: "Environment variables (optional)"; color: Theme.muted; font.pixelSize: 11 }
            TextArea {
                id: envField
                Layout.fillWidth: true
                Layout.preferredHeight: 72
                wrapMode: TextEdit.Wrap
                color: Theme.text
                font.pixelSize: Theme.fontSize
                placeholderText: "one KEY=value per line, for example MINIMAX_API_KEY=..."
                placeholderTextColor: Theme.muted
                background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: envField.activeFocus ? Theme.accent : Theme.border }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "The command is run as it is written, without a shell. A value is kept in the app's config folder and given to that server only; it is never shown again, so write the name on its own to keep the value that is stored. Empty the field to take the variables away."
            }
            Label {
                visible: root.serverError !== ""
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.danger
                font.pixelSize: 11
                text: root.serverError
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Btn { text: "Cancel"; onClicked: serverDialog.close() }
                Btn {
                    text: "Save"
                    primary: true
                    onClicked: {
                        const err = root.editor.saveMcpServer(root.editing, nameField.text, commandField.text, envField.text)
                        if (err !== "") { root.serverError = err; return }
                        root.serverError = ""
                        root.refreshServers()
                        serverDialog.close()
                    }
                }
            }
        }
    }

    Dialog {
        id: removeDialog
        property string serverName: ""
        modal: true
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, Overlay.overlay ? Overlay.overlay.width - 40 : 460)
        title: "Remove the MCP server?"
        background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
        header: Label { text: removeDialog.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "The server " + removeDialog.serverName + " and the variables it was given are removed from the app's config folder."
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Btn { text: "Keep it"; onClicked: removeDialog.close() }
                Btn {
                    text: "Remove"
                    primary: true
                    onClicked: {
                        const err = root.editor.removeMcpServer(removeDialog.serverName)
                        removeDialog.close()
                        if (err !== "") { root.serverError = err; return }
                        root.serverError = ""
                        root.refreshServers()
                    }
                }
            }
        }
    }
}
