import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A video the agent built, as the user needs to read it: a preview you can play and scrub, a
// timeline that shows what comes after what (clips in order, how each one arrives, what is laid
// over them and when, the sound), the properties of what you pick, and the same in words.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    readonly property var videos: editor.videos ? JSON.parse(editor.videos) : []
    property int picked: 0
    readonly property var video: videos.length > picked ? videos[picked] : null
    property real playhead: 0
    property bool playing: false
    property bool loop: true
    property string picture: ""
    // the background preview (every frame drawn small on a worker thread): its status from
    // `editor.previewStatus()`, and whether playing waits for it to get ahead of the playhead
    property var pv: ({})
    property string pvRaw: ""
    property bool buffering: false
    // the preview could not be started (no ffmpeg for the footage, a video that is gone)
    property bool failed: false
    property string previewError: ""
    property real lastTick: 0
    property int shownIdx: -1
    readonly property bool cacheOk: !!video && pv.video === video.id && !pv.stale && pv.total > 0
    readonly property int pct: pv.total > 0 ? Math.floor(100 * pv.ready / pv.total) : 0
    // what is picked on the timeline: "clip" | "overlay" | "audio" | ""
    property string selKind: ""
    property string selId: ""
    property string tab: "details"
    readonly property var thumbCache: ({})

    readonly property string pvState: {
        if (!video) return ""
        if (previewError !== "" || pv.error) return "error"
        if (video.clips.length === 0) return "none"
        if (!pv.video || pv.video !== video.id || pv.stale) return "updating"
        if (!pv.done) return "rendering"
        return buffering ? "buffering" : "ready"
    }
    readonly property bool narrow: width < 420

    onVideoChanged: {
        clearThumbs()
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

    function setStatus(status) {
        const raw = JSON.stringify(status)
        if (raw === pvRaw) return
        pvRaw = raw
        pv = status
    }
    function ensure() {
        if (!video || video.clips.length === 0) { editor.previewStop(); setStatus({}); failed = false; previewError = ""; return }
        if (failed) return // it did not work once; the panel says so
        const status = JSON.parse(editor.previewStart(video.id))
        failed = !!status.error
        previewError = status.error ? status.error : ""
        setStatus(status)
    }
    function refresh() { setStatus(JSON.parse(editor.previewStatus())) }
    function indexAt(ms) { return Math.min(Math.floor(ms * pv.fps / 1000), pv.total - 1) }
    function clearThumbs() { for (const k in thumbCache) delete thumbCache[k] }
    // the cached picture at a moment, for the timeline's thumbnails; "" until it is drawn
    function thumbAt(ms) {
        if (!cacheOk) return ""
        const i = indexAt(ms)
        if (i < 0 || i >= pv.ready) return ""
        if (thumbCache[i] === undefined) thumbCache[i] = editor.previewFrame(i)
        return thumbCache[i]
    }
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
    function stepFrames(n) {
        if (!video) return
        const fps = Math.max(1, video.fps)
        seek((Math.round(playhead * fps / 1000) + n) * 1000 / fps)
    }
    function endOfPlay() {
        if (loop) { playhead = 0; return }
        playing = false
        playhead = video.duration_ms
    }
    // one step of playback: the playhead follows the clock, but never gets ahead of the pictures
    function tick() {
        const now = Date.now()
        const dt = Math.min(now - lastTick, 250)
        lastTick = now
        refresh()
        if (pv.stale) { clearThumbs(); buffering = true; ensure(); return }
        if (!cacheOk) { buffering = !pv.error && previewError === ""; return }
        if (buffering) {
            // start once the first second is drawn (or all of it)
            if (pv.ready >= Math.min(pv.total, pv.fps) || pv.done) buffering = false
            else return
        }
        const next = playhead + dt
        if (next >= video.duration_ms || (pv.done && indexAt(next) >= pv.ready)) { endOfPlay(); return }
        if (indexAt(next) >= pv.ready && !pv.done) { buffering = true; return }
        playhead = next
    }
    function handleKey(e) {
        if (e.key === Qt.Key_Space) playing = !playing
        else if (e.key === Qt.Key_Left) stepFrames(-1)
        else if (e.key === Qt.Key_Right) stepFrames(1)
        else if (e.key === Qt.Key_Home) seek(0)
        else if (e.key === Qt.Key_End) seek(video ? video.duration_ms : 0)
        else return
        e.accepted = true
    }
    function pick(kind, id) { selKind = kind; selId = id }

    function ease(e) { return String(e).replace("_", " ") }
    function seconds(ms) { return (ms / 1000).toFixed(ms % 1000 === 0 ? 0 : 2) + " s" }
    // the properties of what is picked (or of the video), as [label, value] rows
    readonly property var detailRows: {
        const v = video
        if (!v) return []
        const tc = timeline.tc
        if (selKind === "clip") {
            const c = v.clips.find(x => x.id === selId)
            if (c) {
                const rows = [
                    ["Kind", c.footage !== "" ? "Footage" : "Design frame"],
                    ["Shows", c.footage !== "" ? c.footage.replace("media/", "") : c.frame_name],
                    ["Starts", tc(c.start_ms)],
                    ["Ends", tc(c.start_ms + c.duration_ms)],
                    ["Length", seconds(c.duration_ms)]
                ]
                if (c.footage !== "") rows.push(["Footage from", seconds(c.footage_in_ms)])
                rows.push(["Arrives", c.transition_ms > 0
                           ? c.transition.kind.replace("_", " ") + ", " + seconds(c.transition_ms) + ", " + ease(c.transition.ease)
                           : "cut"])
                rows.push(["Zoom", c.motion.scale_from === c.motion.scale_to
                           ? "none"
                           : c.motion.scale_from.toFixed(2) + " to " + c.motion.scale_to.toFixed(2) + ", " + ease(c.motion.ease)])
                return rows
            }
        } else if (selKind === "overlay") {
            const o = v.overlays.find(x => x.id === selId)
            if (o) return [
                ["Kind", "Overlay"],
                ["Starts", tc(o.start_ms)],
                ["Ends", tc(o.start_ms + o.duration_ms)],
                ["Length", seconds(o.duration_ms)],
                ["Enters", o.enter.replace("_", " ")],
                ["Leaves", o.exit.replace("_", " ")],
                ["Fade", seconds(o.fade_ms)],
                ["Ease", ease(o.ease)]
            ]
        } else if (selKind === "audio" && v.audio) {
            const a = v.audio
            return [
                ["Kind", "Sound"],
                ["File", a.file.replace("media/", "")],
                ["From", seconds(a.start_ms)],
                ["Volume", a.gain.toFixed(2)],
                ["Fades in", a.fade_in_ms > 0 ? seconds(a.fade_in_ms) : "no"],
                ["Fades out", a.fade_out_ms > 0 ? seconds(a.fade_out_ms) : "no"]
            ]
        }
        return [
            ["Size", v.width + " × " + v.height],
            ["Rate", v.fps + " fps"],
            ["Length", tc(v.duration_ms) + " (" + seconds(v.duration_ms) + ")"],
            ["Clips", String(v.clips.length)],
            ["Overlays", String(v.overlays.length)],
            ["Sound", v.audio ? v.audio.file.replace("media/", "") : "none"]
        ]
    }
    readonly property bool soundPicked: selKind === "audio" && !!video && !!video.audio

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
            if (root.pv.stale) { root.clearThumbs(); root.ensure() }
            else root.showCached()
        }
    }

    // ---- no video yet ----
    ColumnLayout {
        visible: root.videos.length === 0
        anchors.centerIn: parent
        width: Math.min(parent.width - 2 * Theme.pad, 360)
        spacing: 8
        Icon { Layout.alignment: Qt.AlignHCenter; name: "film"; size: 32; color: Theme.muted }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            color: Theme.text
            font.pixelSize: Theme.fontSize
            font.bold: true
            text: "No video in this workspace yet"
        }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: 12
            text: "Ask the agent in the chat, for example: cut my footage into a 30 second teaser with a title. Add screen recordings or clips to the project's media folder first. The video appears here as a timeline you can play, with what comes after what."
        }
    }

    ColumnLayout {
        visible: root.videos.length > 0
        anchors.fill: parent
        spacing: 0

        // ---- header: which video, how big ----
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 8
            Layout.bottomMargin: 6
            spacing: 8
            ComboBox {
                visible: root.videos.length > 1
                Layout.fillWidth: true
                model: root.videos.map(v => v.name)
                currentIndex: root.picked
                Accessible.name: "Video"
                onActivated: i => { root.playing = false; root.picked = i; root.playhead = 0; root.pick("", "") }
            }
            Label {
                visible: root.videos.length === 1
                Layout.fillWidth: true
                text: root.video ? root.video.name : ""
                color: Theme.text
                font.pixelSize: Theme.fontSize
                font.bold: true
                elide: Text.ElideRight
            }
            Label {
                visible: !root.narrow || root.videos.length === 1
                text: root.video ? root.video.width + "×" + root.video.height + " · " + root.video.fps + " fps" : ""
                color: Theme.muted
                font.pixelSize: 11
            }
        }

        // ---- preview ----
        Rectangle {
            id: stage
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 60
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            color: Theme.canvasBg
            radius: Theme.radius
            border.width: 1
            border.color: Theme.border
            clip: true

            Image {
                anchors.fill: parent
                anchors.margins: 6
                source: root.picture
                fillMode: Image.PreserveAspectFit
                cache: false
                smooth: true
                opacity: root.pvState === "updating" ? 0.55 : 1
            }
            ColumnLayout {
                visible: root.picture === "" && root.pvState !== "error"
                anchors.centerIn: parent
                spacing: 6
                Icon { Layout.alignment: Qt.AlignHCenter; name: "film"; size: 24; color: Theme.muted }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    color: Theme.muted
                    font.pixelSize: 12
                    text: root.pvState === "none" ? "This video has no clips yet. Ask the agent to add some." : "Drawing the first picture"
                }
            }
            Label {
                visible: root.pvState === "error"
                anchors.centerIn: parent
                width: parent.width - 24
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                color: Theme.danger
                font.pixelSize: 12
                text: "Preview stopped: " + (root.previewError || root.pv.error || "")
            }

            // what the preview is doing
            Rectangle {
                visible: root.pvState === "updating" || root.pvState === "rendering" || root.pvState === "buffering" || !!root.pv.capped
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.margins: 6
                width: Math.min(parent.width - 12, statusRow.implicitWidth + 16)
                height: 24
                radius: 12
                color: Qt.rgba(0, 0, 0, 0.7)
                Row {
                    id: statusRow
                    anchors.centerIn: parent
                    spacing: 8
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        color: Theme.text
                        font.pixelSize: 11
                        text: root.pvState === "updating" ? "Updating preview, the video changed"
                            : root.pvState === "rendering" ? "Preparing preview " + root.pct + "%"
                            : root.pvState === "buffering" ? "Buffering " + root.pct + "%"
                            : "Preview holds the first " + (root.pv.ready / Math.max(1, root.pv.fps)).toFixed(1) + " s (memory limit); export has all"
                    }
                    ProgressBar {
                        visible: root.pvState === "rendering" || root.pvState === "buffering"
                        anchors.verticalCenter: parent.verticalCenter
                        width: 70
                        from: 0; to: 100; value: root.pct
                    }
                }
            }
            Label {
                visible: root.pvState === "ready" && !root.narrow
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 8
                text: "Draft preview, silent. Export is full size with sound."
                color: Theme.muted
                font.pixelSize: 10
            }
        }

        // ---- transport ----
        Flow {
            Layout.fillWidth: true
            Layout.margins: 8
            Layout.topMargin: 6
            Layout.bottomMargin: 4
            spacing: 4
            Row {
                spacing: 2
                Btn { iconName: "skip-back"; tip: "Go to start (Home)"; onClicked: root.seek(0) }
                Btn { iconName: "step-back"; tip: "Back one frame (Left)"; onClicked: root.stepFrames(-1) }
                Btn {
                    iconName: root.playing ? "pause" : "play"
                    primary: true
                    tip: root.playing ? "Pause (Space)" : "Play (Space)"
                    onClicked: root.playing = !root.playing
                }
                Btn { iconName: "step-forward"; tip: "Forward one frame (Right)"; onClicked: root.stepFrames(1) }
                Btn { iconName: "skip-forward"; tip: "Go to end (End)"; onClicked: root.seek(root.video ? root.video.duration_ms : 0) }
                Btn { iconName: "repeat"; active: root.loop; tip: root.loop ? "Looping is on" : "Looping is off"; onClicked: root.loop = !root.loop }
            }
            Item { width: 6; height: 1 }
            Label {
                height: Theme.row
                verticalAlignment: Text.AlignVCenter
                text: timeline.tc(root.playhead) + " / " + timeline.tc(root.video ? root.video.duration_ms : 0)
                color: Theme.text
                font.pixelSize: 12
                font.family: Qt.platform.os === "osx" ? "Menlo" : "monospace"
                Accessible.name: "Time " + text
            }
            Item { width: 6; height: 1 }
            Row {
                spacing: 2
                Btn { iconName: "zoom-out"; enabled: timeline.canZoomOut; tip: "Zoom out (Ctrl or Cmd + wheel)"; onClicked: timeline.zoomBy(1 / 1.5) }
                Btn { iconName: "zoom-in"; enabled: timeline.canZoomIn; tip: "Zoom in (Ctrl or Cmd + wheel)"; onClicked: timeline.zoomBy(1.5) }
                Btn { iconName: "maximize"; tip: "Fit the whole video in the window"; onClicked: timeline.fit() }
            }
        }

        // ---- the timeline ----
        VideoTimeline {
            id: timeline
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.preferredHeight: implicitHeight
            video: root.video
            playhead: root.playhead
            selKind: root.selKind
            selId: root.selId
            thumbAt: ms => root.thumbAt(ms)
            focus: true
            onSeek: ms => root.seek(ms)
            onPicked: (kind, id) => root.pick(kind, id)
            onKey: e => root.handleKey(e)
            onFindSound: {
                if (!root.video) return
                root.editor.chatSend("Find a sound (music or a sound effect) on Freesound for the video \"" + root.video.name + "\" (" + root.video.id + "), import it and put it under the video. Tell me the license and the attribution line I have to show.")
            }
        }

        // ---- details and the story ----
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.topMargin: 6
            spacing: 2
            Btn { text: "Details"; active: root.tab === "details"; onClicked: root.tab = "details" }
            Btn { text: "Story"; active: root.tab === "story"; onClicked: root.tab = "story" }
            Item { Layout.fillWidth: true }
            Btn {
                visible: root.soundPicked
                text: "Remove sound"
                onClicked: { root.editor.removeVideoAudio(root.video.id); root.pick("", "") }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 56
            Layout.preferredHeight: 120
            Layout.maximumHeight: 220
            Layout.margins: 8
            Layout.topMargin: 4
            color: Theme.field
            radius: Theme.radius
            border.width: 1
            border.color: Theme.border

            Flickable {
                visible: root.tab === "details"
                anchors.fill: parent
                anchors.margins: 8
                clip: true
                contentWidth: width
                contentHeight: grid.implicitHeight
                ScrollBar.vertical: ScrollBar {}
                GridLayout {
                    id: grid
                    width: parent.width
                    columns: width > 460 ? 4 : 2
                    columnSpacing: 12
                    rowSpacing: 4
                    Repeater {
                        model: root.detailRows.length * 2
                        delegate: Label {
                            required property int index
                            readonly property var row: root.detailRows[Math.floor(index / 2)]
                            readonly property bool isKey: index % 2 === 0
                            Layout.fillWidth: !isKey
                            Layout.preferredWidth: isKey ? 78 : -1
                            Layout.maximumWidth: isKey ? 78 : 100000
                            text: row ? row[index % 2] : ""
                            color: isKey ? Theme.muted : Theme.text
                            font.pixelSize: 11
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                        }
                    }
                    Label {
                        visible: root.selKind === "" && !!root.video
                        Layout.columnSpan: grid.columns
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: 11
                        text: "Pick a clip, overlay or the sound on the timeline to see its properties. The preview is small and silent; the exported MP4 has full size and the footage's sound."
                    }
                    Label {
                        visible: root.soundPicked
                        Layout.columnSpan: grid.columns
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: 11
                        text: "The agent finds sounds; you keep the license and the attribution."
                    }
                }
            }
            ListView {
                visible: root.tab === "story"
                anchors.fill: parent
                anchors.margins: 8
                clip: true
                model: root.video ? root.video.story : []
                spacing: 3
                ScrollBar.vertical: ScrollBar {}
                delegate: Label {
                    required property string modelData
                    required property int index
                    width: ListView.view.width - 12
                    text: (index + 1) + ". " + modelData
                    color: Theme.muted
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    textFormat: Text.PlainText
                }
            }
        }
    }
}
