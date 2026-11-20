import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The end product, clean: what the agent chose to show (show_preview), or what the user picked
// to look at instead. No selection, handles, pins or agent outlines; a frame is drawn as it
// exports, a video plays with the preview machinery the Video panel uses, an image is the file.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var showcase: editor.showcase ? JSON.parse(editor.showcase) : null
    readonly property var shown: showcase ? showcase.preview : null
    readonly property var choices: showcase ? showcase.choices : []
    readonly property bool isVideo: !!shown && shown.kind === "video"
    readonly property var videoInfo: {
        if (!isVideo) return null
        const list = editor.videos ? JSON.parse(editor.videos) : []
        return list.find(v => v.id === shown.target) || null
    }
    readonly property real duration: videoInfo ? Math.max(1, videoInfo.duration_ms) : 1
    readonly property int stage: Math.max(64, Math.min(stageBox.width, stageBox.height) > 0 ? Math.round(Math.max(stageBox.width, stageBox.height) * Screen.devicePixelRatio) : 512)
    property string picture: ""
    property real playhead: 0
    property bool playing: false
    property var pv: ({})
    property real lastTick: 0
    property string previewError: ""
    readonly property bool cacheOk: !!shown && pv.video === shown.target && !pv.stale && pv.total > 0

    function whoChose() {
        if (!shown) return ""
        if (shown.chosen_by === "user") return "You chose this"
        if (shown.chosen_by === "agent") return "Chosen by the agent" + (shown.note ? ": " + shown.note : "")
        return "Nothing chosen yet: showing the " + (shown.kind === "video" ? "newest video" : "open frame")
    }
    function render() {
        if (!shown || !shown.exists) { picture = ""; return }
        if (shown.kind === "frame") picture = editor.renderFrameImage(shown.target, stage)
        else if (shown.kind === "image") picture = shown.file ? "file://" + shown.file : ""
        else if (!playing) picture = editor.videoPreview(shown.target, playhead, Math.min(stage, 960))
    }
    function indexAt(ms) { return Math.min(Math.floor(ms * pv.fps / 1000), pv.total - 1) }
    function ensure() {
        if (!isVideo || !videoInfo || videoInfo.clips.length === 0) return
        const status = JSON.parse(editor.previewStart(shown.target))
        previewError = status.error ? status.error : ""
        pv = status
    }
    function tick() {
        const now = Date.now()
        const dt = Math.min(now - lastTick, 250)
        lastTick = now
        pv = JSON.parse(editor.previewStatus())
        if (pv.stale || !pv.video) { ensure(); return }
        if (!cacheOk) return
        const next = playhead + dt
        if (next >= duration || (pv.done && indexAt(next) >= pv.ready)) { playhead = 0; return }
        if (indexAt(next) >= pv.ready) return
        playhead = next
        picture = editor.previewFrame(indexAt(next))
    }
    function seek(ms) { playing = false; playhead = Math.max(0, Math.min(ms, duration)); renderTimer.restart() }

    readonly property string shownKey: shown ? shown.kind + ":" + shown.target : ""
    onShownKeyChanged: { playing = false; playhead = 0 }
    onShownChanged: renderTimer.restart()
    onStageChanged: renderTimer.restart()
    onPlayingChanged: {
        if (playing) {
            if (playhead >= duration) playhead = 0
            lastTick = Date.now()
            ensure()
        } else {
            renderTimer.restart()
        }
    }
    // the document changed (an agent or the user edited a frame): draw it again
    Connections { target: root.editor; function onLogChanged() { renderTimer.restart() } }
    Component.onCompleted: render()

    Timer { id: renderTimer; interval: 150; onTriggered: root.render() }
    Timer { interval: root.pv.fps ? Math.round(1000 / root.pv.fps) : 66; repeat: true; running: root.playing; onTriggered: root.tick() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        ColumnLayout {
            visible: !root.shown
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 8
            Icon { name: "eye"; size: 22; color: Theme.muted }
            Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.text; font.pixelSize: Theme.fontSize; font.bold: true; text: "Nothing to show yet" }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "This is where the finished work appears, clean and without editing handles: the frame, video or image the agent decides you should see. Until the agent shows something, it shows your newest video, or the open frame."
            }
        }

        RowLayout {
            visible: !!root.shown
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.topMargin: 8
            Icon { name: root.shown ? (root.shown.kind === "video" ? "film" : root.shown.kind === "image" ? "image" : "frame") : "frame"; size: 16; color: Theme.muted }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: Theme.text
                    font.pixelSize: Theme.fontSize
                    font.bold: true
                    text: root.shown ? (root.shown.title || root.shown.name || "") : ""
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: Theme.muted
                    font.pixelSize: 11
                    text: root.whoChose() + (root.shown && root.shown.width ? " · " + Math.round(root.shown.width) + "×" + Math.round(root.shown.height) : "")
                }
            }
        }
        RowLayout {
            visible: !!root.shown
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            ComboBox {
                id: chooser
                Layout.fillWidth: true
                Accessible.name: "Show something else"
                model: root.choices
                textRole: "name"
                displayText: root.shown ? (root.shown.name || "") : ""
                delegate: ItemDelegate {
                    required property var modelData
                    width: chooser.width
                    text: modelData.kind + ": " + modelData.name
                }
                onActivated: i => root.editor.setPreviewOverride(root.choices[i].kind, root.choices[i].target)
            }
            Btn {
                visible: !!root.shown && root.shown.chosen_by === "user"
                text: "Let the agent decide"
                iconName: "undo-2"
                onClicked: root.editor.clearPreviewOverride()
            }
        }

        Item {
            id: stageBox
            visible: !!root.shown
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 10
            Image {
                anchors.fill: parent
                source: root.picture
                fillMode: Image.PreserveAspectFit
                cache: false
                smooth: true
            }
            Label {
                anchors.centerIn: parent
                visible: !!root.shown && !root.shown.exists
                color: Theme.danger
                text: "This is no longer in the project."
            }
        }

        RowLayout {
            visible: root.isVideo && !!root.videoInfo
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 6
            Btn {
                iconName: root.playing ? "pause" : "play"
                tip: root.playing ? "Pause" : "Play"
                onClicked: root.playing = !root.playing
            }
            Slider {
                Layout.fillWidth: true
                from: 0
                to: root.duration
                value: root.playhead
                Accessible.name: "Position in the video"
                onMoved: root.seek(value)
            }
            Label { color: Theme.muted; font.pixelSize: 11; text: (root.playhead / 1000).toFixed(1) + " / " + (root.duration / 1000).toFixed(1) + " s" }
        }
        Label {
            visible: root.isVideo && root.previewError !== ""
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            wrapMode: Text.WordWrap
            color: Theme.danger
            font.pixelSize: 11
            text: "Preview stopped: " + root.previewError
        }
        Label {
            visible: root.isVideo
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 6
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: "The preview is small and silent. The exported MP4 has full size and the footage's sound."
        }
        Item { visible: !root.shown; Layout.fillHeight: true }
    }
}
