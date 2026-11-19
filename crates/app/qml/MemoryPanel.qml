import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Pictures and videos the agent may use. They are copied into the project's media folder.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    property string openNote: ""
    function reloadNotes() {
        notesModel.sync(JSON.parse(editor.memoryNotes()))
        if (openNote !== "" && !notesModel.hasKey(openNote)) openNote = ""
    }
    JsonModel { id: memoryModel }
    JsonModel { id: notesModel }
    readonly property var memory: editor.memory ? JSON.parse(editor.memory) : []
    onMemoryChanged: { memoryModel.sync(memory); reloadNotes() }
    Component.onCompleted: { memoryModel.sync(memory); reloadNotes() }
    FileDialog {
        id: memoryDialog
        title: "Add pictures for the agent"
        fileMode: FileDialog.OpenFiles
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp)"]
        onAccepted: { for (const u of selectedFiles) root.editor.importToMemory(u.toString()) }
    }

        ColumnLayout {
        anchors.fill: parent
            spacing: 0
            Label {
                Layout.fillWidth: true
                Layout.margins: 10
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "Pictures you add here wait for the agent: it sees them (list_media) and uses them when the work needs one. They are copied into this project's media folder and never placed on the design by you."
            }
            RowLayout {
                Layout.leftMargin: 10
                Layout.rightMargin: 10
                Btn { text: "Add pictures…"; primary: true; enabled: root.editor.hasProject; onClicked: memoryDialog.open() }
                Label { text: "or drop them on the window"; color: Theme.muted; font.pixelSize: 11 }
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.topMargin: 8
                clip: true
                model: memoryModel
                spacing: 4
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property string json
                    readonly property var modelData: JSON.parse(json)
                    x: 10
                    width: ListView.view.width - 20
                    height: 58
                    radius: Theme.radius
                    color: Theme.panelRaised
                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 8
                        Image {
                            Layout.preferredWidth: 46
                            Layout.preferredHeight: 46
                            source: modelData.file ? "file://" + modelData.file : ""
                            sourceSize.width: 92
                            sourceSize.height: 92
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 1
                            Label { text: modelData.name; color: Theme.text; font.pixelSize: 12; elide: Text.ElideMiddle; Layout.fillWidth: true }
                            Label { text: Math.round(modelData.bytes / 1024) + " KB · " + (modelData.used ? "used in the design" : "not used yet"); color: Theme.muted; font.pixelSize: 11 }
                        }
                    }
                }
            }
            Label {
                visible: memoryModel.count === 0
                Layout.fillWidth: true
                Layout.margins: 10
                text: "Nothing here yet."
                color: Theme.muted
                font.pixelSize: 12
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                Label {
                    Layout.fillWidth: true
                    text: "Notes for the agent"
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    font.bold: true
                }
                Btn { text: "Refresh"; onClicked: root.reloadNotes() }
            }
            Label {
                Layout.fillWidth: true
                Layout.leftMargin: 10
                Layout.rightMargin: 10
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "Text the agent keeps in .opendesigner/memory/ between tasks (brand facts, lessons, decisions). Track that folder in git; never put secrets in it."
            }
            Repeater {
                model: notesModel
                delegate: Label {
                    required property string json
                    readonly property var modelData: JSON.parse(json)
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    elide: Text.ElideRight
                    color: root.openNote === modelData.name ? Theme.text : Theme.muted
                    font.pixelSize: 12
                    text: modelData.name + " · " + Math.max(1, Math.round(modelData.bytes / 1024)) + " KB · " + modelData.title
                    MouseArea { anchors.fill: parent; onClicked: root.openNote = modelData.name }
                }
            }
            Label {
                visible: notesModel.count === 0
                Layout.margins: 10
                text: "No notes yet."
                color: Theme.muted
                font.pixelSize: 12
            }
            TextArea {
                id: noteText
                visible: root.openNote !== ""
                Layout.fillWidth: true
                Layout.preferredHeight: 140
                Layout.margins: 10
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                color: Theme.text
                font.pixelSize: 11
                property var note: root.openNote !== "" ? JSON.parse(root.editor.memoryNote(root.openNote)) : ({})
                text: note.text !== undefined ? note.text : (note.error !== undefined ? note.error : "")
                background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: Theme.border }
            }
        }
}
