import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A video the agent built, as the user needs to read it: a preview you can play and scrub, a
// timeline that shows what comes after what (clips in order, how each one arrives, what is laid
// over them and when), and the same thing in words.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var videos: editor.videos ? JSON.parse(editor.videos) : []
    property int picked: 0
    readonly property var video: videos.length > picked ? videos[picked] : null
    property real playhead: 0
    property bool playing: false
    property string picture: ""
    // the background preview (every frame drawn small on a worker thread): its status from
    // `editor.previewStatus()`, and whether playing waits for it to get ahead of the playhead
    property var pv: ({})
    property bool buffering: false
    // the preview could not be started (no ffmpeg for the footage, a video that is gone)
    property bool failed: false
    property string previewError: ""
    property real lastTick: 0
    property int shownIdx: -1
    readonly property bool cacheOk: !!video && pv.video === video.id && !pv.stale && pv.total > 0
    readonly property int pct: pv.total > 0 ? Math.floor(100 * pv.ready / pv.total) : 0
    // pixels per second of the timeline
    property real zoom: 70

    JsonModel { id: clipsModel }
    JsonModel { id: overlaysModel }
    onVideoChanged: {
        clipsModel.sync(video ? video.clips : [])
        overlaysModel.sync(video ? video.overlays : [])
        previewTimer.restart()
        ensureTimer.restart()
    }
    onPlayheadChanged: {
        // while playing the picture comes from the cache only; a single frame is drawn on
        // demand only when the user scrubs
        if (showCached()) return
        if (!playing) previewTimer.restart()
    }
    onPlayingChanged: {
        if (playing) {
            if (video && playhead >= video.duration_ms) playhead = 0
            lastTick = Date.now()
            buffering = true
            ensure()
        } else {
            buffering = false
        }
    }
    Component.onCompleted: ensure()
    Component.onDestruction: editor.previewStop()
    function ensure() {
        if (!video || video.clips.length === 0) { editor.previewStop(); pv = ({}); failed = false; previewError = ""; return }
        if (failed) return // it did not work once; the panel says so
        const status = JSON.parse(editor.previewStart(video.id))
        failed = !!status.error
        previewError = status.error ? status.error : ""
        pv = status
    }
    function refresh() { pv = JSON.parse(editor.previewStatus()) }
    function indexAt(ms) { return Math.min(Math.floor(ms * pv.fps / 1000), pv.total - 1) }
    // the cached picture of the playhead, when it is drawn
    function showCached() {
        if (!cacheOk) return false
        const i = indexAt(playhead)
        if (i >= pv.ready) return false
        if (i !== shownIdx) { picture = editor.previewFrame(i); shownIdx = i }
        return true
    }
    // a single picture drawn on demand (scrubbing before the cache has got there)
    function show() {
        if (showCached()) return
        if (playing) return // never wait for a render while playing
        shownIdx = -1
        picture = video ? editor.videoPreview(video.id, playhead, 480) : ""
    }
    function seek(ms) { playing = false; playhead = Math.max(0, Math.min(ms, video ? video.duration_ms : 0)) }
    // one step of playback: the playhead follows the clock, but never gets ahead of the pictures
    function tick() {
        const now = Date.now()
        const dt = Math.min(now - lastTick, 250)
        lastTick = now
        refresh()
        if (pv.stale) { buffering = true; ensure(); return }
        if (!cacheOk) { buffering = !pv.error && previewError === ""; return }
        if (buffering) {
            // start once the first second is drawn (or all of it)
            if (pv.ready >= Math.min(pv.total, pv.fps) || pv.done) buffering = false
            else return
        }
        const next = playhead + dt
        if (next >= video.duration_ms || (pv.done && indexAt(next) >= pv.ready)) { playhead = 0; return }
        if (indexAt(next) >= pv.ready && !pv.done) { buffering = true; return }
        playhead = next
    }
    // overlays that overlap in time go on lanes below each other
    function laneOf(i) {
        const list = video ? video.overlays : []
        const lanes = []
        for (let k = 0; k <= i; ++k) {
            const o = list[k]
            let lane = 0
            while (lanes[lane] !== undefined && lanes[lane] > o.start_ms) lane++
            lanes[lane] = o.start_ms + o.duration_ms
            if (k === i) return lane
        }
        return 0
    }
    readonly property int laneCount: {
        let n = 1
        for (let i = 0; video && i < video.overlays.length; ++i) n = Math.max(n, laneOf(i) + 1)
        return n
    }

    Timer { id: previewTimer; interval: 120; onTriggered: root.show() }
    Timer { id: ensureTimer; interval: 500; onTriggered: root.ensure() }
    // playback at the preview's rate, by the clock
    Timer {
        interval: root.pv.fps ? Math.round(1000 / root.pv.fps) : 66
        repeat: true; running: root.playing && !!root.video
        onTriggered: root.tick()
    }
    // follows the worker while it draws, so the first picture and the progress show
    Timer {
        interval: 150; repeat: true
        running: !!root.video && !root.playing && !!root.pv.video && !root.pv.done
        onTriggered: {
            root.refresh()
            if (root.pv.stale) root.ensure()
            else root.showCached()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        Label {
            visible: root.videos.length === 0
            Layout.fillWidth: true
            Layout.margins: 10
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: Theme.fontSize
            text: "No video in this workspace yet. Ask the agent for one (for example: cut my footage into a 30 second teaser with a title). It appears here as a timeline you can play, with what comes after what."
        }
        ComboBox {
            visible: root.videos.length > 1
            Layout.fillWidth: true
            Layout.margins: 10
            model: root.videos.map(v => v.name)
            onActivated: i => { root.playing = false; root.picked = i; root.playhead = 0 }
        }
        Image {
            visible: !!root.video
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredHeight: 200
            Layout.preferredWidth: root.video ? Math.max(60, 200 * root.video.width / root.video.height) : 0
            source: root.picture
            fillMode: Image.PreserveAspectFit
            cache: false
        }
        RowLayout {
            visible: !!root.video
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Btn { text: root.playing ? "Pause" : "Play"; iconName: root.playing ? "pause" : "play"; onClicked: root.playing = !root.playing }
            Slider {
                Layout.fillWidth: true
                from: 0
                to: root.video ? Math.max(1, root.video.duration_ms) : 1
                value: root.playhead
                onMoved: root.seek(value)
            }
            Label {
                text: (root.playhead / 1000).toFixed(1) + " / " + (root.video ? (root.video.duration_ms / 1000).toFixed(1) : "0") + " s"
                color: Theme.muted
                font.pixelSize: 11
            }
        }
        RowLayout {
            visible: !!root.video && root.video.clips.length > 0 && !!root.pv.video && !root.pv.stale && (!root.pv.done || root.buffering)
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            ProgressBar { Layout.fillWidth: true; from: 0; to: 100; value: root.pct }
            Label {
                color: Theme.muted
                font.pixelSize: 11
                text: root.pv.done
                      ? (root.buffering ? "Waiting for the preview" : "Preview ready")
                      : (root.playing && root.buffering ? "Buffering " : "Preparing preview ") + root.pct + "%"
            }
        }
        Label {
            visible: !!root.video && (root.previewError !== "" || !!root.pv.error)
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            wrapMode: Text.WordWrap
            color: Theme.danger
            font.pixelSize: 11
            text: "Preview stopped: " + (root.previewError || root.pv.error)
        }
        Label {
            visible: !!root.video && !!root.pv.capped
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: root.pv.capped ? "The preview holds only the first " + (root.pv.ready / root.pv.fps).toFixed(1) + " s (memory limit); the export has all of it." : ""
        }
        Label {
            visible: !!root.video
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 11
            text: "The preview is small and silent. The exported MP4 has full size and the footage's sound."
        }
        Label {
            visible: !!root.video
            Layout.leftMargin: 10
            text: root.video ? root.video.width + "×" + root.video.height + " · " + root.video.fps + " fps · " + root.video.clips.length + " clips · " + root.video.overlays.length + " overlays" : ""
            color: Theme.muted
            font.pixelSize: 11
        }

        // the sound under the picture
        Rectangle {
            visible: !!(root.video && root.video.audio)
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            implicitHeight: soundRow.implicitHeight + 12
            radius: 4
            color: Theme.field
            border.width: 1
            border.color: Theme.border
            RowLayout {
                id: soundRow
                anchors.fill: parent
                anchors.margins: 6
                spacing: 8
                Icon { visible: !!(root.video && root.video.audio); name: "music"; size: 14; color: Theme.text }
                Label {
                    Layout.fillWidth: true
                    text: root.video && root.video.audio
                        ? root.video.audio.file.replace("media/", "")
                          + " · from " + (root.video.audio.start_ms / 1000).toFixed(1) + " s"
                          + " · volume " + root.video.audio.gain.toFixed(2)
                          + (root.video.audio.fade_in_ms > 0 ? " · fades in " + (root.video.audio.fade_in_ms / 1000).toFixed(1) + " s" : "")
                          + (root.video.audio.fade_out_ms > 0 ? " · fades out " + (root.video.audio.fade_out_ms / 1000).toFixed(1) + " s" : "")
                        : ""
                    color: Theme.text
                    font.pixelSize: 11
                    elide: Text.ElideRight
                }
                Btn { text: "Remove"; onClicked: root.editor.removeVideoAudio(root.video.id) }
            }
        }
        RowLayout {
            visible: !!root.video
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Btn {
                text: "Find a sound"
                onClicked: {
                    if (!root.video) return
                    root.editor.chatSend("Find a sound (music or a sound effect) on Freesound for the video \"" + root.video.name + "\" (" + root.video.id + "), import it and put it under the video. Tell me the license and the attribution line I have to show.")
                }
            }
            Label {
                Layout.fillWidth: true
                text: "The agent finds it; you keep the license and the attribution."
                color: Theme.muted
                font.pixelSize: 11
                wrapMode: Text.WordWrap
            }
        }

        // ---- the timeline ----
        Flickable {
            id: flick
            visible: !!root.video
            Layout.fillWidth: true
            Layout.preferredHeight: 44 + 56 + 30 * root.laneCount + 18
            clip: true
            contentWidth: root.video ? root.video.duration_ms / 1000 * root.zoom + 40 : 0
            contentHeight: height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }

            // seconds ruler
            Repeater {
                model: root.video ? Math.floor(root.video.duration_ms / 1000) + 1 : 0
                delegate: Item {
                    required property int index
                    x: 10 + index * root.zoom
                    Rectangle { width: 1; height: 8; color: Theme.border; y: 14 }
                    Label { y: 0; x: 3; text: index + " s"; color: Theme.muted; font.pixelSize: 10 }
                }
            }
            Label { x: 6; y: 26; text: "Clips, in order"; color: Theme.muted; font.pixelSize: 10 }
            Repeater {
                model: clipsModel
                delegate: Item {
                    id: clipItem
                    required property string json
                    readonly property var c: JSON.parse(json)
                    x: 10 + c.start_ms / 1000 * root.zoom
                    y: 42
                    width: Math.max(4, c.duration_ms / 1000 * root.zoom)
                    height: 56
                    Rectangle {
                        anchors.fill: parent
                        radius: 4
                        color: clipItem.c.footage !== "" ? Qt.rgba(0.35, 0.69, 1, 0.22) : Qt.rgba(0.95, 0.79, 0.51, 0.22)
                        border.width: 1
                        border.color: clipItem.c.footage !== "" ? Theme.agent : Theme.accent
                    }
                    // how this clip arrives: the stretch it overlaps the one before
                    Rectangle {
                        visible: clipItem.c.transition_ms > 0
                        width: clipItem.c.transition_ms / 1000 * root.zoom
                        height: parent.height
                        radius: 4
                        color: Qt.rgba(1, 1, 1, 0.16)
                        border.width: 1
                        border.color: Qt.rgba(1, 1, 1, 0.4)
                        Icon { anchors.centerIn: parent; name: "arrow-right-left"; size: 12; color: Theme.text; visible: parent.width > 14 }
                    }
                    Column {
                        x: (clipItem.c.transition_ms / 1000 * root.zoom) + 5
                        y: 4
                        width: parent.width - x - 4
                        spacing: 1
                        Row {
                            width: parent.width
                            spacing: 4
                            Icon { name: clipItem.c.footage !== "" ? "play" : "frame"; size: 11; color: Theme.text; anchors.verticalCenter: parent.verticalCenter }
                            Label { width: parent.width - 15; text: clipItem.c.footage !== "" ? clipItem.c.footage.replace("media/", "") : clipItem.c.frame_name; color: Theme.text; font.pixelSize: 11; elide: Text.ElideRight }
                        }
                        Label { width: parent.width; visible: clipItem.c.transition_ms > 0; text: clipItem.c.transition.kind.replace("_", " ") + " " + (clipItem.c.transition_ms / 1000).toFixed(1) + " s"; color: Theme.muted; font.pixelSize: 10; elide: Text.ElideRight }
                        Label { width: parent.width; visible: clipItem.c.motion.scale_from !== clipItem.c.motion.scale_to; text: "zoom " + clipItem.c.motion.scale_from.toFixed(2) + " → " + clipItem.c.motion.scale_to.toFixed(2); color: Theme.muted; font.pixelSize: 10; elide: Text.ElideRight }
                    }
                }
            }
            Label { x: 6; y: 102; text: "Laid over them"; color: Theme.muted; font.pixelSize: 10; visible: root.video && root.video.overlays.length > 0 }
            Repeater {
                model: overlaysModel
                delegate: Rectangle {
                    id: overlayItem
                    required property string json
                    required property int index
                    readonly property var o: JSON.parse(json)
                    x: 10 + o.start_ms / 1000 * root.zoom
                    y: 118 + root.laneOf(index) * 30
                    width: Math.max(4, o.duration_ms / 1000 * root.zoom)
                    height: 26
                    radius: 4
                    color: Qt.rgba(0.8, 0.5, 0.95, 0.22)
                    border.width: 1
                    border.color: "#b07ad8"
                    Label { anchors.fill: parent; anchors.margins: 4; text: overlayItem.o.enter.replace("_", " ") + (overlayItem.o.exit !== overlayItem.o.enter ? " / " + overlayItem.o.exit.replace("_", " ") : "") + " · " + overlayItem.o.frame; color: Theme.text; font.pixelSize: 10; elide: Text.ElideRight }
                }
            }
            // the playhead
            Rectangle { x: 10 + root.playhead / 1000 * root.zoom; y: 0; width: 2; height: flick.height; color: Theme.accent }
            TapHandler { onTapped: p => root.seek((p.position.x - 10) / root.zoom * 1000) }
            WheelHandler {
                acceptedModifiers: Qt.ControlModifier | Qt.MetaModifier
                onWheel: e => root.zoom = Math.max(20, Math.min(400, root.zoom * Math.pow(1.002, e.angleDelta.y)))
            }
        }

        Label {
            visible: !!root.video
            Layout.leftMargin: 10
            text: "What happens, in order"
            color: Theme.text
            font.pixelSize: 11
            font.bold: true
        }
        ListView {
            visible: !!root.video
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.video ? root.video.story : []
            spacing: 3
            ScrollBar.vertical: ScrollBar {}
            delegate: Label {
                required property string modelData
                required property int index
                x: 10
                width: ListView.view.width - 20
                text: (index + 1) + ". " + modelData
                color: Theme.muted
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
            }
        }
    }
}
