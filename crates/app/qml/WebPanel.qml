import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A running web service of the approved run plan, shown at phone, tablet or desktop width. The
// page loads only when its service is ready; the status comes from the same overview the Run
// panel polls.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    property var overview: ({ open: false })
    property int widthPreset: 2
    property string chosen: ""
    readonly property var presets: [
        { label: "Phone", width: 390 },
        { label: "Tablet", width: 820 },
        { label: "Fit", width: 0 }
    ]
    readonly property var urls: overview.urls || []
    readonly property var services: Array.isArray(overview.services) ? overview.services : []
    readonly property var current: urls.find(u => u.name === chosen) || (urls.length > 0 ? urls[0] : null)
    readonly property var status: current ? services.find(s => s.name === current.name) : null
    readonly property bool ready: !!status && status.ready === true
    readonly property bool approved: overview.approved === true

    Timer {
        interval: 1500
        repeat: true
        running: root.visible && root.editor.hasProject
        triggeredOnStart: true
        onTriggered: root.overview = JSON.parse(root.editor.codeOverview())
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 8
            spacing: 6
            ComboBox {
                Layout.preferredWidth: 130
                visible: root.urls.length > 1
                model: root.urls.map(u => u.name)
                onActivated: i => root.chosen = root.urls[i].name
            }
            Label {
                Layout.fillWidth: true
                text: root.current ? root.current.url : "no web service"
                color: Theme.muted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
            Repeater {
                model: root.presets
                delegate: Btn {
                    required property int index
                    required property var modelData
                    text: modelData.label
                    active: root.widthPreset === index
                    onClicked: root.widthPreset = index
                }
            }
            Btn { iconName: "repeat"; tip: "Reload"; enabled: root.ready && loader.item !== null; onClicked: loader.item.reload() }
            Btn { text: "Open in browser"; enabled: !!root.current; onClicked: Qt.openUrlExternally(root.current.url) }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.canvasBg

            Label {
                anchors.centerIn: parent
                width: parent.width - 40
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                color: Theme.muted
                visible: !root.ready || loader.status === Loader.Error
                text: !root.editor.hasProject ? "Open a folder to use Code mode."
                    : !root.current ? "No web service yet. In Code mode the agent proposes a run plan with a dev server; you approve it in the Run panel."
                    : !root.approved ? "Approve the run plan in the Run panel to start " + root.current.name + "."
                    : loader.status === Loader.Error ? "The web view is not available in this build. Use Open in browser."
                    : root.status && root.status.state === "exited" ? root.current.name + " exited. Read its log in the Run panel."
                    : "Waiting for " + root.current.name + " to listen on its port…"
            }

            Loader {
                id: loader
                anchors.horizontalCenter: parent.horizontalCenter
                height: parent.height
                width: root.presets[root.widthPreset].width > 0 ? Math.min(parent.width, root.presets[root.widthPreset].width) : parent.width
                active: root.ready
                source: "qrc:/qt/qml/org/opendesigner/app/qml/WebFrame.qml"
                onLoaded: item.address = root.current.url
            }
            Binding { target: loader.item; property: "address"; value: root.current ? root.current.url : ""; when: loader.item !== null }
        }
    }
}
