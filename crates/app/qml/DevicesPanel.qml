import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The iOS Simulators and Android emulators on this computer, and the screen of one of them,
// refreshed about once a second while Live is on. "Save to media" puts the current screen in the
// design project, ready for a store screenshot or a 3D device capture.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var found: editor.simulators !== "" ? JSON.parse(editor.simulators) : ({})
    readonly property var list: found.simulators || []
    property string selected: ""
    property bool live: true
    readonly property var current: list.find(s => s.id === selected) || null
    readonly property bool showing: root.visible && editor.hasProject && current !== null && current.state === "booted"

    Timer {
        interval: 6000
        repeat: true
        running: root.visible && root.editor.hasProject
        triggeredOnStart: true
        onTriggered: root.editor.refreshSimulators()
    }
    Timer {
        interval: 1000
        repeat: true
        running: root.showing && root.live
        triggeredOnStart: true
        onTriggered: root.editor.captureSimulator(root.selected)
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 10

        ColumnLayout {
            Layout.preferredWidth: 230
            Layout.fillHeight: true
            spacing: 6
            RowLayout {
                Layout.fillWidth: true
                Label { Layout.fillWidth: true; text: "Simulators"; color: Theme.text; font.bold: true }
                Btn { text: "Refresh"; onClicked: root.editor.refreshSimulators() }
            }
            Label {
                Layout.fillWidth: true
                visible: !!root.found.error || (root.list.length === 0 && root.editor.simulators !== "")
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: root.found.error ? root.found.error : "None found. iOS Simulators need Xcode; Android emulators need the Android SDK (ANDROID_HOME)."
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 4
                model: root.list
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    width: ListView.view.width
                    height: 46
                    radius: Theme.radius
                    color: root.selected === modelData.id ? Theme.panelRaised : "transparent"
                    border.width: 1
                    border.color: root.selected === modelData.id ? Theme.accent : Theme.border
                    MouseArea { anchors.fill: parent; onClicked: root.selected = row.modelData.id }
                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 6
                        Rectangle { width: 8; height: 8; radius: 4; color: row.modelData.state === "booted" ? Theme.success : Theme.muted }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            Label { Layout.fillWidth: true; text: row.modelData.name; color: Theme.text; elide: Text.ElideRight }
                            Label { text: (row.modelData.platform === "ios" ? "iOS " + row.modelData.os : "Android") + " · " + row.modelData.state; color: Theme.muted; font.pixelSize: 10 }
                        }
                        Btn { visible: row.modelData.state !== "booted"; text: "Boot"; onClicked: root.editor.bootSimulator(row.modelData.id) }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 6
            RowLayout {
                Layout.fillWidth: true
                CheckBox { text: "Live"; checked: root.live; onToggled: root.live = checked }
                Btn { text: "Update now"; enabled: root.showing; onClicked: root.editor.captureSimulator(root.selected) }
                Item { Layout.fillWidth: true }
                Btn { text: "Save to media"; enabled: root.showing; onClicked: root.editor.saveSimulatorScreen(root.selected) }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: Theme.canvasBg
                radius: Theme.radius
                Image {
                    anchors.fill: parent
                    anchors.margins: 6
                    fillMode: Image.PreserveAspectFit
                    cache: false
                    asynchronous: true
                    visible: root.showing
                    source: root.showing ? root.editor.simulatorPicture : ""
                }
                Label {
                    anchors.centerIn: parent
                    width: parent.width - 30
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    visible: !root.showing
                    text: !root.editor.hasProject ? "Open a folder first."
                        : !root.current ? "Choose a simulator on the left."
                        : "Boot " + root.current.name + " to see its screen here."
                }
            }
        }
    }
}
