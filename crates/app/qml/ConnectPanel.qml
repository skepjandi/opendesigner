import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Connecting an agent (what to paste into it), the skill that teaches it OpenDesigner, and what
// it may read on this computer.
Rectangle {
    id: root
    required property var editor
    signal toast(string text)
    color: Theme.panel

    property var skillRows: []
    readonly property var recipes: editor.mcpRunning ? JSON.parse(editor.connectRecipes()) : []
    readonly property var recipe: recipes.length > recipeBox.currentIndex ? recipes[recipeBox.currentIndex] : null
    property string skillResult: ""
    function refreshSkill() { skillRows = JSON.parse(editor.skillStatus()) }
    Component.onCompleted: refreshSkill()

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
