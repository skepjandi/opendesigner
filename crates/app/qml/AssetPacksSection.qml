import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The downloadable asset packs (a download needs the user's click) and the MCP servers worth
// adding. Both are lists the user acts on; nothing here downloads by itself.
ColumnLayout {
    id: root
    required property var editor
    // the names of the servers already added (from the Connect tab's list)
    property var addedNames: []
    signal serverAdded()

    property var packs: []
    property var requests: []
    property real installedBytes: 0
    property real capBytes: 0
    property var recommended: []

    function mb(bytes) { return (bytes / 1048576).toFixed(bytes < 10485760 ? 1 : 0) + " MB" }
    function refresh() {
        const v = JSON.parse(editor.assetPacks())
        packs = v.packs || []
        requests = v.requests || []
        installedBytes = v.installed_bytes || 0
        capBytes = v.cap_bytes || 0
    }
    function downloading() {
        for (let i = 0; i < packs.length; i++) if (packs[i].state === "downloading") return true
        return false
    }
    Component.onCompleted: { recommended = JSON.parse(editor.recommendedServers()); refresh() }
    Timer { interval: 700; repeat: true; running: root.downloading(); onTriggered: root.refresh() }

    spacing: 6

    Label { text: "Asset packs"; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 12
        text: "Icons, backgrounds and shapes the agent can draw without downloading anything are built in. Packs below add more icon sets and Kenney's CC0 pictures and sounds. They are kept compressed in this app's own folder, never in a project (a project only gets the files an agent uses), and you can unload them at any time."
    }
    Label {
        visible: root.requests.length > 0
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.agent
        font.pixelSize: 12
        text: "An agent asked for: " + root.requests.join(", ") + ". Nothing is downloaded until you press Download."
    }
    Repeater {
        model: root.packs
        delegate: RowLayout {
            required property var modelData
            Layout.fillWidth: true
            spacing: 8
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label {
                    text: modelData.name + " · " + modelData.licence
                    color: Theme.text
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                Label {
                    text: modelData.about
                    color: Theme.muted
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }
                Label {
                    text: modelData.state === "installed" ? "installed · " + root.mb(modelData.stored_bytes) + " on disk · " + modelData.files + " files"
                        : modelData.state === "downloading" ? "downloading…"
                        : modelData.state === "failed" ? "failed: " + modelData.error
                        : "download " + root.mb(modelData.download_bytes) + (modelData.state === "requested" ? " · asked for by an agent" : "")
                    color: modelData.state === "installed" ? Theme.success : modelData.state === "failed" ? Theme.danger : Theme.muted
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }
            }
            Btn {
                visible: modelData.state === "available" || modelData.state === "requested" || modelData.state === "failed"
                text: "Download"
                primary: modelData.state === "requested"
                onClicked: { root.editor.installAssetPack(modelData.id); root.refresh() }
            }
            Btn {
                visible: modelData.state === "requested"
                text: "Dismiss"
                onClicked: { root.editor.dismissAssetRequest(modelData.id); root.refresh() }
            }
            Btn {
                visible: modelData.state === "installed"
                text: "Unload"
                onClicked: { root.editor.unloadAssetPack(modelData.id); root.refresh() }
            }
        }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 11
        text: "Packs on disk: " + root.mb(root.installedBytes) + " of at most " + root.mb(root.capBytes) + "."
    }

    Label { text: "Recommended servers"; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true; Layout.topMargin: 8 }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 12
        text: "Optional servers that search 3D assets. They run on your computer when the agent uses them (npx or uvx must be installed), and you can remove them again above. OpenDesigner cannot place 3D models yet."
    }
    Repeater {
        model: root.recommended
        delegate: RowLayout {
            required property var modelData
            readonly property bool added: root.addedNames.indexOf(modelData.name) >= 0
            Layout.fillWidth: true
            spacing: 8
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label { text: modelData.name + " · " + modelData.licence; color: Theme.text; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
                Label { text: modelData.about; color: Theme.muted; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Label { text: modelData.command; color: Theme.muted; font.pixelSize: 11; elide: Text.ElideMiddle; Layout.fillWidth: true }
            }
            Btn {
                text: parent.added ? "Added" : "Add"
                enabled: !parent.added
                onClicked: {
                    const err = root.editor.saveMcpServer("", modelData.name, modelData.command, modelData.env)
                    if (err === "") root.serverAdded()
                }
            }
        }
    }
}
