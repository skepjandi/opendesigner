import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// What changed: a restore point before each agent turn, and the log of every change with who made it.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    JsonModel { id: checkpointsModel }
    JsonModel { id: logModel }
    readonly property var log: editor.log ? JSON.parse(editor.log) : []
    readonly property var checkpoints: editor.checkpoints ? JSON.parse(editor.checkpoints) : []
    onCheckpointsChanged: checkpointsModel.sync(checkpoints.slice().reverse().slice(0, 6))
    onLogChanged: logModel.sync(log)
    Component.onCompleted: {
        checkpointsModel.sync(checkpoints.slice().reverse().slice(0, 6))
        logModel.sync(log)
    }

        ColumnLayout {
        anchors.fill: parent
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
                model: checkpointsModel
                delegate: RowLayout {
                    required property string json
                    readonly property var modelData: JSON.parse(json)
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
                model: logModel
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property string json
                    readonly property var modelData: JSON.parse(json)
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
}
