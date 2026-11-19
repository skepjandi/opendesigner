import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Pictures and videos the agent may use. They are copied into the project's media folder.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    JsonModel { id: memoryModel }
    readonly property var memory: editor.memory ? JSON.parse(editor.memory) : []
    onMemoryChanged: memoryModel.sync(memory)
    Component.onCompleted: memoryModel.sync(memory)
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
        }
}
