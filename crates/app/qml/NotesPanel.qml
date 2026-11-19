import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// The notes you pinned for the agent. It reads the open ones before it acts and resolves them
// when it is done.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    JsonModel { id: notesModel }
    readonly property var notes: editor.annotations ? JSON.parse(editor.annotations) : []
    onNotesChanged: notesModel.sync(notes)
    Component.onCompleted: notesModel.sync(notes)

        ColumnLayout {
        anchors.fill: parent
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
                model: notesModel
                spacing: 6
                topMargin: 8
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    id: card
                    required property string json
                    readonly property var modelData: JSON.parse(json)
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
}
