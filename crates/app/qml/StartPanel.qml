import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Where to work, which agent to start and what to ask it first. Used on launch and from the
// project menu. The folder is usually a repository: the agent starts there (so it can run the
// app and take screenshots) and the design lives in an `opendesigner/` folder inside it.
ColumnLayout {
    id: root
    required property var editor
    signal started()

    property string folderUrl: ""
    property var info: null
    readonly property var presets: JSON.parse(editor.chatPresets())
    readonly property var recent: JSON.parse(editor.recentProjects())
    readonly property bool agentChosen: agentBox.currentIndex >= 0 && presets[agentBox.currentIndex].command !== "" || customCommand.text.trim() !== ""
    readonly property string command: presets[agentBox.currentIndex].command !== "" ? presets[agentBox.currentIndex].command : customCommand.text.trim()
    spacing: 12

    function choose(url) {
        folderUrl = url
        info = JSON.parse(editor.inspectFolder(url))
    }

    Component.onCompleted: {
        const first = presets.findIndex(p => p.found)
        agentBox.currentIndex = first >= 0 ? first : 0
        const last = editor.chatLastCommand()
        const at = presets.findIndex(p => p.command === last)
        if (last !== "" && at >= 0) agentBox.currentIndex = at
        if (recent.length > 0) choose("file://" + recent[0].folder)
    }

    FolderDialog {
        id: folderDialog
        title: "Choose the folder you are working in"
        onAccepted: root.choose(selectedFolder.toString())
    }

    Label { text: "Where are you working?"; color: Theme.text; font.pixelSize: 15; font.bold: true }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 12
        text: "Choose the folder of your project (a repository, say). The agent starts there, so it can run your app and take screenshots; the designs, notes and exports are kept in an opendesigner folder inside it."
    }
    RowLayout {
        Layout.fillWidth: true
        Label {
            Layout.fillWidth: true
            text: root.info ? root.editor.localPath(root.folderUrl) : "No folder chosen"
            color: root.info ? Theme.text : Theme.muted
            elide: Text.ElideMiddle
        }
        Btn { text: "Choose folder…"; primary: !root.info; onClicked: folderDialog.open() }
    }
    Label {
        Layout.fillWidth: true
        visible: root.info !== null
        wrapMode: Text.WordWrap
        font.pixelSize: 11
        color: root.info && root.info.found === "none" ? Theme.accent : Theme.success
        text: !root.info ? "" :
              root.info.found === "project" ? "This folder is an OpenDesigner project; it will be opened."
            : root.info.found === "nested" ? "An OpenDesigner project is already in opendesigner/ here; it will be opened."
            : "A new, empty OpenDesigner project will be set up in opendesigner/ here. The agent decides what to make."
    }

    Label {
        visible: root.recent.length > 0
        text: "Recent"
        color: Theme.muted
        font.pixelSize: 11
        font.bold: true
    }
    Flow {
        Layout.fillWidth: true
        spacing: 6
        Repeater {
            model: root.recent
            delegate: Btn {
                required property var modelData
                text: modelData.name
                onClicked: root.choose("file://" + modelData.folder)
                ToolTip.visible: hovered
                ToolTip.text: modelData.folder
            }
        }
    }

    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

    Label { text: "Your agent"; color: Theme.text; font.pixelSize: 15; font.bold: true }
    RowLayout {
        Layout.fillWidth: true
        ComboBox {
            id: agentBox
            Layout.preferredWidth: 190
            model: root.presets.map(p => p.found ? p.label + "  (found)" : p.label)
        }
        Field {
            id: customCommand
            Layout.fillWidth: true
            visible: root.presets[agentBox.currentIndex].command === ""
            placeholderText: "command that starts your agent"
        }
        Label {
            Layout.fillWidth: true
            visible: root.presets[agentBox.currentIndex].command !== ""
            text: root.presets[agentBox.currentIndex].command
            color: Theme.muted
            font.pixelSize: 11
            elide: Text.ElideRight
        }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 11
        text: "You stay signed in to the agent yourself. It is started on this computer when you press the button, and its work appears here live."
    }

    Label { text: "What should it make first?"; color: Theme.text; font.pixelSize: 15; font.bold: true }
    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: 96
        TextArea {
            id: prompt
            wrapMode: TextEdit.Wrap
            color: Theme.text
            font.pixelSize: Theme.fontSize
            placeholderText: "For example: start the app, take screenshots of its main screens and make the App Store set. Or: cut the footage I imported into a 30 second teaser with titles."
            placeholderTextColor: Theme.muted
            enabled: root.agentChosen
            background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: prompt.activeFocus ? Theme.accent : Theme.border }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Item { Layout.fillWidth: true }
        Btn {
            text: root.agentChosen && root.info ? "Open and start the agent" : "Open"
            primary: true
            enabled: root.info !== null && root.info.exists
            onClicked: {
                if (!root.editor.openWorkspace(root.folderUrl, true)) return
                if (root.agentChosen) root.editor.chatStartWith(root.command, prompt.text)
                root.started()
            }
        }
    }
}
