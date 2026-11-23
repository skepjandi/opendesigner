import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Code mode: the stack the project is built with, the processes the agent proposes to run, and
// the user's approval of them. The agent starts and stops services only after Approve; each runs
// in its own tmux session, so it can also be watched in a terminal.
Rectangle {
    id: root
    required property var editor
    signal askAgent(string text)
    signal toast(string text)
    color: Theme.panel

    property var overview: ({ open: false })
    property string openLog: ""
    property string logText: ""
    readonly property bool codeMode: overview.mode === "code"
    readonly property var plan: overview.plan || null
    readonly property bool approved: overview.approved === true
    readonly property var services: Array.isArray(overview.services) ? overview.services : []
    readonly property string statusError: overview.services && overview.services.error ? overview.services.error : ""

    function reload() {
        overview = JSON.parse(editor.codeOverview())
        if (openLog !== "") logText = editor.serviceLog(openLog)
    }
    function statusOf(name) { return services.find(s => s.name === name) || null }
    function stateLabel(s) {
        if (!s) return "not started"
        if (s.state === "running") return s.kind === "web" ? (s.ready ? "running · ready" : "running · starting") : "running"
        if (s.state === "exited") return "exited with code " + s.code
        return "stopped"
    }
    function stateColor(s) {
        if (!s || s.state === "stopped") return Theme.muted
        if (s.state === "exited") return s.code === 0 ? Theme.muted : Theme.danger
        return s.kind === "web" && !s.ready ? Theme.accent : Theme.success
    }
    function copy(text) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        toast("Copied")
    }
    TextEdit { id: clipboard; visible: false }

    Timer {
        interval: 1500
        repeat: true
        running: root.visible && root.editor.hasProject
        triggeredOnStart: true
        onTriggered: root.reload()
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        ColumnLayout {
            width: root.width - 20
            x: 10
            spacing: 10

            Item { Layout.preferredHeight: 2 }

            Label {
                Layout.fillWidth: true
                visible: !root.editor.hasProject
                wrapMode: Text.WordWrap
                color: Theme.muted
                text: "Open a folder to use Code mode."
            }

            // ---- assets mode ----
            ColumnLayout {
                Layout.fillWidth: true
                visible: root.editor.hasProject && !root.codeMode
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    text: "This folder is in Assets mode: the agent makes designs, store assets and videos and starts no processes. Code mode lets it build and run the project itself, and designs and assets stay available."
                }
                Btn { text: "Switch to Code mode"; primary: true; onClicked: { if (root.editor.setCodeMode(true)) root.reload() } }
            }

            // ---- code mode ----
            ColumnLayout {
                Layout.fillWidth: true
                visible: root.editor.hasProject && root.codeMode
                spacing: 10

                Label { text: "Stack"; color: Theme.text; font.bold: true }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: root.overview.stack ? Theme.text : Theme.muted
                    text: root.overview.stack
                          ? root.overview.stack.name + " · " + root.overview.stack.language + (root.overview.stack.framework ? " · " + root.overview.stack.framework : "") + "  (" + root.overview.stack.target + ", chosen by " + (root.overview.stack.decided_by === "user" ? "you" : "the agent with you") + ")"
                          : "No stack yet. Tell the agent what to use, or let it ask you a few questions and propose options."
                }
                Label {
                    Layout.fillWidth: true
                    visible: !!(root.overview.stack && root.overview.stack.notes)
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: 11
                    text: root.overview.stack ? root.overview.stack.notes : ""
                }
                Btn {
                    visible: !root.overview.stack
                    text: "Ask me about the stack"
                    onClicked: root.askAgent("Help me choose the stack for this project. Read get_code_project and get_playbook code, ask me your questions a few at a time, then propose two or three options and wait for my choice.")
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

                Label { text: "Run plan"; color: Theme.text; font.bold: true }
                Label {
                    Layout.fillWidth: true
                    visible: !root.plan
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    text: "The agent proposes the processes the project needs (a dev server, an API) and you approve them here before anything runs."
                }
                Label {
                    Layout.fillWidth: true
                    visible: !!root.plan && !root.approved
                    wrapMode: Text.WordWrap
                    color: Theme.accent
                    text: "These commands run on your computer, in this project's folder, in tmux. Read them, then approve. If the agent changes the plan you approve again."
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.statusError !== ""
                    wrapMode: Text.WordWrap
                    color: Theme.danger
                    text: root.statusError
                }

                Repeater {
                    model: root.plan ? root.plan.services : []
                    delegate: Rectangle {
                        id: row
                        required property var modelData
                        readonly property var status: root.statusOf(modelData.name)
                        readonly property var url: (root.overview.urls || []).find(u => u.name === modelData.name)
                        readonly property var attach: (root.overview.attach || []).find(a => a.name === modelData.name)
                        Layout.fillWidth: true
                        implicitHeight: body.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.panelRaised
                        border.width: 1
                        border.color: Theme.border

                        ColumnLayout {
                            id: body
                            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 8 }
                            spacing: 6
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Rectangle { width: 8; height: 8; radius: 4; color: root.approved ? root.stateColor(row.status) : Theme.muted }
                                Label { text: row.modelData.name; color: Theme.text; font.bold: true }
                                Label {
                                    Layout.fillWidth: true
                                    text: root.approved ? root.stateLabel(row.status) : row.modelData.kind + (row.modelData.port ? " · port " + row.modelData.port : "")
                                    color: Theme.muted
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                }
                                Btn { visible: root.approved && (!row.status || row.status.state !== "running"); iconName: "play"; tip: "Start"; onClicked: root.editor.runService(row.modelData.name, "start") }
                                Btn { visible: root.approved && !!row.status && row.status.state === "running"; iconName: "repeat"; tip: "Restart"; onClicked: root.editor.runService(row.modelData.name, "restart") }
                                Btn { visible: root.approved && !!row.status && row.status.state === "running"; iconName: "x"; tip: "Stop"; onClicked: root.editor.runService(row.modelData.name, "stop") }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: "$ " + row.modelData.command + (row.modelData.cwd !== "." ? "   (in " + row.modelData.cwd + ")" : "")
                                color: Theme.text
                                font.family: Qt.platform.os === "osx" ? "Menlo" : "monospace"
                                font.pixelSize: 11
                                wrapMode: Text.WrapAnywhere
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                visible: root.approved
                                spacing: 6
                                Label { visible: !!row.url; text: row.url ? row.url.url : ""; color: Theme.agent; font.pixelSize: 11 }
                                Item { Layout.fillWidth: true }
                                Btn { text: root.openLog === row.modelData.name ? "Hide log" : "Log"; onClicked: { root.openLog = root.openLog === row.modelData.name ? "" : row.modelData.name; root.reload() } }
                                Btn { text: "Copy tmux command"; onClicked: root.copy(row.attach ? row.attach.command : "") }
                            }
                            ScrollView {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 160
                                visible: root.openLog === row.modelData.name
                                TextArea {
                                    readOnly: true
                                    text: root.logText
                                    color: Theme.text
                                    font.family: Qt.platform.os === "osx" ? "Menlo" : "monospace"
                                    font.pixelSize: 11
                                    wrapMode: TextEdit.WrapAnywhere
                                    background: Rectangle { radius: 4; color: Theme.field; border.width: 1; border.color: Theme.border }
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    visible: !!root.plan
                    spacing: 8
                    Btn {
                        visible: !root.approved
                        text: "Approve and start"
                        primary: true
                        onClicked: { root.editor.approveRunPlan(); root.reload() }
                    }
                    Btn {
                        visible: root.approved
                        text: "Withdraw approval and stop"
                        onClicked: { root.editor.revokeRunPlan(); root.openLog = ""; root.reload() }
                    }
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }
                Btn { text: "Back to Assets mode"; onClicked: { if (root.editor.setCodeMode(false)) root.reload() } }
            }

            Item { Layout.preferredHeight: 10 }
        }
    }
}
