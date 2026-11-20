import QtQuick
import QtQuick.Controls

// The tracks of a video: a time ruler, a playhead across every track, and three labelled tracks
// (clips, overlays, sound) that scroll sideways while their headers stay put. It only draws and
// reports what the user did (`seek`, `picked`); the panel owns the playhead and the selection.
Item {
    id: tl

    property var video: null
    property real playhead: 0
    property string selKind: ""
    property string selId: ""
    // function(ms) -> image source of the cached preview picture at that time, or ""
    property var thumbAt: null
    signal seek(real ms)
    signal picked(string kind, string id)
    signal findSound()
    signal key(var event)
    Keys.onPressed: e => tl.key(e)

    // pixels per second; follows the window until the user zooms by hand
    property real zoom: 70
    property bool userZoomed: false
    readonly property real minZoom: 4
    readonly property real maxZoom: 800
    readonly property bool canZoomIn: zoom < maxZoom - 0.01
    readonly property bool canZoomOut: zoom > minZoom + 0.01

    readonly property real duration: video ? video.duration_ms : 0
    readonly property real fps: video && video.fps > 0 ? video.fps : 30
    readonly property int headerW: width < 420 ? 34 : 96
    readonly property int rulerH: 26
    readonly property int clipH: 54
    readonly property int laneH: 24
    readonly property int audioH: 32
    readonly property real pad: 12
    readonly property real viewW: Math.max(0, width - headerW)

    // overlays that overlap in time go on lanes below each other
    readonly property var laneOf: {
        const list = video ? video.overlays : []
        const ends = []
        const out = []
        for (let i = 0; i < list.length; ++i) {
            let lane = 0
            while (ends[lane] !== undefined && ends[lane] > list[i].start_ms) lane++
            ends[lane] = list[i].start_ms + list[i].duration_ms
            out.push(lane)
        }
        return out
    }
    readonly property int laneCount: Math.max(1, laneOf.length ? Math.max.apply(null, laneOf) + 1 : 1)
    readonly property int videoTrackH: clipH + 10
    readonly property int overlayTrackH: laneCount * laneH + 10
    readonly property int audioTrackH: audioH + 8
    readonly property int tracksH: videoTrackH + overlayTrackH + audioTrackH

    implicitHeight: rulerH + tracksH
    clip: true

    readonly property color footageFill: "#1f3447"
    readonly property color frameFill: "#3a3123"
    readonly property color overlayFill: "#35264a"
    readonly property color overlayEdge: "#b07ad8"
    readonly property color audioFill: "#1f3a2c"

    function pad2(n) { return n < 10 ? "0" + n : String(n) }
    function tc(ms) {
        const total = Math.max(0, Math.round(ms))
        const s = Math.floor(total / 1000)
        const f = Math.min(Math.floor(fps) - 1, Math.floor((total % 1000) * fps / 1000))
        return pad2(Math.floor(s / 60)) + ":" + pad2(s % 60) + ":" + pad2(Math.max(0, f))
    }
    function seconds(ms) { return (ms / 1000).toFixed(ms % 1000 === 0 ? 0 : 1) + " s" }
    function xOf(ms) { return pad + ms / 1000 * zoom }
    function msAtContent(x) { return Math.max(0, Math.min(duration, (x - pad) / zoom * 1000)) }
    function msAtView(x) { return msAtContent(x + hf.contentX) }
    function clampX(x) { return Math.max(0, Math.min(x, hf.contentWidth - viewW)) }

    function zoomTo(z, anchorViewX) {
        const next = Math.max(minZoom, Math.min(maxZoom, z))
        const anchorMs = (anchorViewX + hf.contentX - pad) / zoom * 1000
        zoom = next
        hf.contentX = clampX(pad + anchorMs / 1000 * next - anchorViewX)
    }
    // buttons zoom around the playhead when it is on screen, else around the middle
    function zoomBy(factor) {
        userZoomed = true
        const px = xOf(playhead) - hf.contentX
        zoomTo(zoom * factor, px >= 0 && px <= viewW ? px : viewW / 2)
    }
    function fit() {
        userZoomed = false
        applyFit()
    }
    function applyFit() {
        if (duration <= 0 || viewW <= 0) return
        const z = (viewW - 2 * pad - 6) / (duration / 1000)
        zoom = Math.max(minZoom, Math.min(userZoomed ? maxZoom : 160, z))
        hf.contentX = 0
    }
    function followPlayhead() {
        const x = xOf(playhead)
        if (x < hf.contentX + 8 || x > hf.contentX + viewW - 24)
            hf.contentX = clampX(x - viewW * 0.25)
    }

    onDurationChanged: if (!userZoomed) applyFit()
    onViewWChanged: if (!userZoomed) applyFit()
    onPlayheadChanged: followPlayhead()
    Component.onCompleted: if (!userZoomed) applyFit()

    // ---- headers (fixed) ----
    Rectangle {
        x: 0; y: 0; width: tl.headerW; height: tl.rulerH
        color: Theme.panelRaised
        border.width: 0
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
    }
    Column {
        id: headers
        x: 0; y: tl.rulerH
        width: tl.headerW
        Repeater {
            model: [
                { icon: "film", label: "Video", h: tl.videoTrackH },
                { icon: "layers", label: "Overlays", h: tl.overlayTrackH },
                { icon: "music", label: "Sound", h: tl.audioTrackH }
            ]
            delegate: Rectangle {
                id: head
                required property var modelData
                width: tl.headerW
                height: modelData.h
                color: Theme.panelRaised
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
                Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.border }
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.horizontalCenter: tl.headerW < 60 ? parent.horizontalCenter : undefined
                    x: tl.headerW < 60 ? 0 : 10
                    spacing: 6
                    Icon { name: head.modelData.icon; size: 14; color: Theme.muted; anchors.verticalCenter: parent.verticalCenter }
                    Label {
                        visible: tl.headerW >= 60
                        text: head.modelData.label
                        color: Theme.muted
                        font.pixelSize: 11
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Accessible.role: Accessible.StaticText
                Accessible.name: modelData.label + " track"
            }
        }
    }

    // ---- everything that scrolls sideways ----
    Item {
        id: view
        x: tl.headerW; y: 0
        width: tl.viewW; height: tl.height
        clip: true

        // ruler
        Rectangle {
            id: ruler
            width: parent.width; height: tl.rulerH
            color: Theme.panelRaised
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            // the first tick spacing that leaves at least 80 px between labels
            readonly property real stepMs: {
                const steps = [100, 200, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000, 600000]
                for (let i = 0; i < steps.length; ++i)
                    if (steps[i] / 1000 * tl.zoom >= 80) return steps[i]
                return 600000
            }
            readonly property real stepPx: stepMs / 1000 * tl.zoom
            readonly property int first: Math.max(0, Math.floor((hf.contentX - tl.pad) / stepPx))
            readonly property int count: Math.ceil(view.width / stepPx) + 2
            Repeater {
                model: ruler.count
                delegate: Item {
                    id: tick
                    required property int index
                    readonly property real t: (ruler.first + index) * ruler.stepMs
                    visible: t <= tl.duration
                    x: tl.pad + t / 1000 * tl.zoom - hf.contentX
                    height: tl.rulerH
                    Rectangle { y: tl.rulerH - 11; width: 1; height: 10; color: Theme.muted }
                    Label { x: 4; y: 2; text: tl.tc(tick.t); color: Theme.muted; font.pixelSize: 10; font.family: Qt.platform.os === "osx" ? "Menlo" : "monospace" }
                    Repeater {
                        model: 4
                        delegate: Rectangle {
                            required property int index
                            visible: tick.t + (index + 1) * ruler.stepMs / 5 <= tl.duration
                            x: (index + 1) * ruler.stepPx / 5
                            y: tl.rulerH - 6
                            width: 1; height: 5
                            color: Theme.border
                        }
                    }
                }
            }
            MouseArea {
                anchors.fill: parent
                preventStealing: true
                cursorShape: Qt.PointingHandCursor
                onPressed: m => { tl.forceActiveFocus(); tl.seek(tl.msAtView(m.x)) }
                onPositionChanged: m => { if (pressed) tl.seek(tl.msAtView(m.x)) }
            }
        }

        // tracks
        Flickable {
            id: hf
            y: tl.rulerH
            width: parent.width
            height: tl.tracksH
            contentWidth: Math.max(view.width, tl.pad + tl.duration / 1000 * tl.zoom + 40)
            contentHeight: tl.tracksH
            interactive: false
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded; height: 8 }

            // track backgrounds; a press on empty track scrubs and clears the selection
            Rectangle { y: 0; width: hf.contentWidth; height: tl.videoTrackH; color: Theme.field }
            Rectangle { y: tl.videoTrackH; width: hf.contentWidth; height: tl.overlayTrackH; color: Theme.panel }
            Rectangle { y: tl.videoTrackH + tl.overlayTrackH; width: hf.contentWidth; height: tl.audioTrackH; color: Theme.field }
            Rectangle { y: tl.videoTrackH - 1; width: hf.contentWidth; height: 1; color: Theme.border }
            Rectangle { y: tl.videoTrackH + tl.overlayTrackH - 1; width: hf.contentWidth; height: 1; color: Theme.border }
            // the end of the video
            Rectangle { x: tl.xOf(tl.duration); width: Math.max(0, hf.contentWidth - x); height: hf.contentHeight; color: Qt.rgba(0, 0, 0, 0.35) }
            MouseArea {
                width: hf.contentWidth; height: hf.contentHeight
                preventStealing: true
                onPressed: m => { tl.forceActiveFocus(); tl.picked("", ""); tl.seek(tl.msAtContent(m.x)) }
                onPositionChanged: m => { if (pressed) tl.seek(tl.msAtContent(m.x)) }
            }

            // clips
            Repeater {
                model: tl.video ? tl.video.clips : []
                delegate: Item {
                    id: clipItem
                    required property var modelData
                    required property int index
                    readonly property var c: modelData
                    readonly property bool isFootage: c.footage !== ""
                    readonly property bool selected: tl.selKind === "clip" && tl.selId === c.id
                    readonly property real zoneW: Math.min(width, c.transition_ms / 1000 * tl.zoom)
                    x: tl.xOf(c.start_ms) + 1
                    y: 5
                    width: Math.max(6, c.duration_ms / 1000 * tl.zoom - 2)
                    height: tl.clipH
                    z: selected ? 2 : 1

                    Rectangle {
                        anchors.fill: parent
                        radius: 5
                        color: clipItem.isFootage ? tl.footageFill : tl.frameFill
                        border.width: 1
                        border.color: clipItem.isFootage ? Theme.agent : Theme.accentStrong
                    }
                    // pictures from the preview, when it has drawn them
                    Item {
                        anchors.fill: parent
                        anchors.margins: 1
                        clip: true
                        readonly property real tileW: Math.max(tl.clipH * (tl.video ? tl.video.width / Math.max(1, tl.video.height) : 1.6), width / 24)
                        Repeater {
                            model: Math.min(24, Math.ceil(parent.width / parent.tileW))
                            delegate: Image {
                                required property int index
                                x: index * parent.tileW
                                width: parent.tileW
                                height: parent.height
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                                cache: false
                                opacity: 0.8
                                sourceSize.height: 96
                                source: tl.thumbAt
                                        ? tl.thumbAt(Math.round((clipItem.c.start_ms + (index + 0.5) * parent.tileW / tl.zoom * 1000) / 250) * 250)
                                        : ""
                            }
                        }
                    }
                    // darken the lower edge so the labels read on any picture
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 1
                        radius: 4
                        gradient: Gradient {
                            GradientStop { position: 0.45; color: "transparent" }
                            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.7) }
                        }
                    }
                    // the stretch shared with the clip before it
                    Rectangle {
                        visible: clipItem.zoneW > 1
                        width: clipItem.zoneW
                        height: parent.height
                        radius: 5
                        border.width: 1
                        border.color: Qt.rgba(1, 1, 1, 0.55)
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.06) }
                            GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0.38) }
                        }
                        Icon { anchors.centerIn: parent; name: "arrow-right-left"; size: 12; color: Theme.text; visible: parent.width > 18 }
                    }
                    Rectangle {
                        x: clipItem.zoneW + 4
                        y: 4
                        width: Math.min(nameRow.implicitWidth + 8, Math.max(0, clipItem.width - x - 4))
                        height: 16
                        radius: 3
                        color: Qt.rgba(0, 0, 0, 0.6)
                        visible: width > 20
                        clip: true
                        Row {
                            id: nameRow
                            x: 4
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 4
                            Icon { name: clipItem.isFootage ? "film" : "frame"; size: 11; color: Theme.text; anchors.verticalCenter: parent.verticalCenter }
                            Label {
                                text: clipItem.isFootage ? clipItem.c.footage.replace("media/", "") : clipItem.c.frame_name
                                color: Theme.text
                                font.pixelSize: 10
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                    }
                    Label {
                        x: clipItem.zoneW + 6
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 4
                        visible: clipItem.width - clipItem.zoneW > 36
                        text: tl.seconds(clipItem.c.duration_ms)
                        color: Theme.text
                        font.pixelSize: 10
                    }
                    Label {
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 4
                        visible: clipItem.c.transition_ms > 0 && clipItem.width - clipItem.zoneW > 110
                        text: clipItem.c.transition.kind.replace("_", " ") + " " + tl.seconds(clipItem.c.transition_ms)
                        color: Theme.muted
                        font.pixelSize: 10
                    }
                    // selection and the trim handles
                    Rectangle {
                        visible: clipItem.selected
                        anchors.fill: parent
                        radius: 5
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.accent
                    }
                    MouseArea {
                        anchors.fill: parent
                        preventStealing: true
                        onPressed: m => {
                            tl.forceActiveFocus()
                            tl.picked("clip", clipItem.c.id)
                            tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x))
                        }
                        onPositionChanged: m => { if (pressed) tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x)) }
                    }
                }
            }

            // overlays
            Repeater {
                model: tl.video ? tl.video.overlays : []
                delegate: Rectangle {
                    id: overlayItem
                    required property var modelData
                    required property int index
                    readonly property var o: modelData
                    readonly property bool selected: tl.selKind === "overlay" && tl.selId === o.id
                    x: tl.xOf(o.start_ms) + 1
                    y: tl.videoTrackH + 5 + (tl.laneOf[index] || 0) * tl.laneH
                    width: Math.max(6, o.duration_ms / 1000 * tl.zoom - 2)
                    height: tl.laneH - 3
                    radius: 5
                    color: tl.overlayFill
                    border.width: selected ? 2 : 1
                    border.color: selected ? Theme.accent : tl.overlayEdge
                    z: selected ? 2 : 1
                    clip: true
                    Row {
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        visible: overlayItem.width > 30
                        Icon { name: "layers"; size: 11; color: Theme.text; anchors.verticalCenter: parent.verticalCenter }
                        Label {
                            text: "Overlay " + (overlayItem.index + 1) + (overlayItem.width > 150
                                  ? " · " + overlayItem.o.enter.replace("_", " ") + (overlayItem.o.exit !== overlayItem.o.enter ? " / " + overlayItem.o.exit.replace("_", " ") : "")
                                  : "")
                            color: Theme.text
                            font.pixelSize: 10
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        preventStealing: true
                        onPressed: m => {
                            tl.forceActiveFocus()
                            tl.picked("overlay", overlayItem.o.id)
                            tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x))
                        }
                        onPositionChanged: m => { if (pressed) tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x)) }
                    }
                }
            }
            Label {
                visible: !!tl.video && tl.video.overlays.length === 0
                x: hf.contentX + 10
                y: tl.videoTrackH + (tl.overlayTrackH - height) / 2
                text: "No titles or overlays"
                color: Theme.muted
                font.pixelSize: 11
            }

            // sound
            Rectangle {
                id: soundBar
                readonly property var a: tl.video ? tl.video.audio : null
                visible: !!a
                x: a ? tl.xOf(a.start_ms) + 1 : 0
                y: tl.videoTrackH + tl.overlayTrackH + 5
                width: a ? Math.max(6, (tl.duration - a.start_ms) / 1000 * tl.zoom - 2) : 0
                height: tl.audioH
                radius: 5
                color: tl.audioFill
                border.width: tl.selKind === "audio" ? 2 : 1
                border.color: tl.selKind === "audio" ? Theme.accent : Theme.success
                clip: true
                // the fades, drawn from the start and (assuming the sound runs to the end of the video) the end
                Rectangle {
                    visible: !!soundBar.a && soundBar.a.fade_in_ms > 0
                    width: soundBar.a ? Math.min(soundBar.width, soundBar.a.fade_in_ms / 1000 * tl.zoom) : 0
                    height: parent.height
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.6) }
                        GradientStop { position: 1; color: "transparent" }
                    }
                }
                Rectangle {
                    visible: !!soundBar.a && soundBar.a.fade_out_ms > 0
                    readonly property real w: soundBar.a ? Math.min(soundBar.width, soundBar.a.fade_out_ms / 1000 * tl.zoom) : 0
                    x: soundBar.width - w
                    width: w
                    height: parent.height
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: "transparent" }
                        GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.6) }
                    }
                }
                Row {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 5
                    Icon { name: "music"; size: 12; color: Theme.text; anchors.verticalCenter: parent.verticalCenter }
                    Label {
                        text: soundBar.a ? soundBar.a.file.replace("media/", "") : ""
                        color: Theme.text
                        font.pixelSize: 10
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    preventStealing: true
                    onPressed: m => {
                        tl.forceActiveFocus()
                        tl.picked("audio", "")
                        tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x))
                    }
                    onPositionChanged: m => { if (pressed) tl.seek(tl.msAtContent(mapToItem(hf.contentItem, m.x, 0).x)) }
                }
            }
            Row {
                visible: !!tl.video && !tl.video.audio
                x: hf.contentX + 8
                y: tl.videoTrackH + tl.overlayTrackH + (tl.audioTrackH - height) / 2
                spacing: 8
                Label { text: "No sound"; color: Theme.muted; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
                Btn {
                    text: "Find a sound"
                    iconName: "music"
                    height: 22
                    font.pixelSize: 11
                    tip: "Ask the agent to find a sound on Freesound"
                    onClicked: tl.findSound()
                }
            }
        }

        // playhead, over the ruler and every track
        Item {
            id: playheadLine
            readonly property real px: tl.xOf(tl.playhead) - hf.contentX
            visible: px > -8 && px < view.width + 8
            x: px
            y: 0
            height: view.height
            width: 0
            z: 10
            Rectangle { x: -1; y: 0; width: 2; height: parent.height; color: Theme.accent }
            Rectangle { x: -6; y: 0; width: 12; height: 12; radius: 3; color: Theme.accent }
        }

        WheelHandler {
            acceptedModifiers: Qt.NoModifier
            onWheel: e => {
                const dx = e.pixelDelta.x !== 0 ? e.pixelDelta.x : e.angleDelta.x
                const dy = e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y
                hf.contentX = tl.clampX(hf.contentX - (Math.abs(dx) > Math.abs(dy) ? dx : dy))
            }
        }
        WheelHandler {
            acceptedModifiers: Qt.ControlModifier | Qt.MetaModifier
            onWheel: e => {
                tl.userZoomed = true
                tl.zoomTo(tl.zoom * Math.pow(1.002, e.angleDelta.y), point.position.x)
            }
        }
        PinchHandler {
            id: pinch
            target: null
            property real startZoom: 70
            onActiveChanged: if (active) startZoom = tl.zoom
            onActiveScaleChanged: {
                if (!active) return
                tl.userZoomed = true
                tl.zoomTo(startZoom * activeScale, centroid.position.x)
            }
        }
    }
}
