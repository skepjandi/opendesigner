import QtQuick
import QtQuick.Controls

// The active frame, rendered by the Rust core, with the editing overlays on top: selection (one
// or many) and resize handles, snapping guides, the outlines of what an agent changed, the user's
// pinned notes, rulers and the drag ghost. Pan and zoom move the picture without asking the core to
// redraw; the core redraws when the document or the zoom settles. Every edit is a call to the
// editor: this file only turns pointer gestures into those calls.
Item {
    id: root
    required property var editor
    property string tool: "select"
    // false shows the clean design: no selection, agent outlines, pins or markers over it
    property bool overlays: true
    // session view state, set by the canvas toolbar
    property bool snapping: true
    property bool rulers: true
    property bool pixelGrid: true
    property real zoom: 0.25
    property real panX: 40
    property real panY: 40
    property bool spaceDown: false
    // the union rectangle (frame coordinates) shown while dragging or resizing the selection
    property var live: null
    // alignment guides to draw while snapping: {axis: "x"|"y", pos, from, to}
    property var guides: []
    // where the pointer is, in frame coordinates
    property point pointer: Qt.point(0, 0)
    readonly property real minZoom: 0.02
    readonly property real maxZoom: 32
    readonly property real rulerSize: 20
    readonly property color guideColor: "#ff4d94"
    readonly property var defaultSizes: ({ rect: [160, 120], ellipse: [160, 160], text: [240, 60], frame: [400, 300] })

    // the orbit gesture on a 3D device: its angles while dragging (a ghost until released)
    property var orbit: null
    readonly property real degreesPerPixel: 0.4

    signal requestPin(string nodeId, real x, real y, real w, real h)
    signal noteClicked(int noteId)
    signal toolFinished()

    readonly property var frameSize: editor.frameSize && editor.frameSize !== "null" ? JSON.parse(editor.frameSize) : null
    readonly property real fw: frameSize ? frameSize.width : 0
    readonly property real fh: frameSize ? frameSize.height : 0
    readonly property var selBounds: editor.selectionBounds && editor.selectionBounds !== "null" ? JSON.parse(editor.selectionBounds) : null
    readonly property var selIds: editor.selection ? JSON.parse(editor.selection) : []
    readonly property var selRects: editor.selectionRects ? JSON.parse(editor.selectionRects) : []
    readonly property bool multi: selIds.length > 1
    readonly property var node: editor.inspector && editor.inspector !== "null" ? JSON.parse(editor.inspector) : null
    readonly property bool canResize: !multi && !!node && node.kind !== "group"
    readonly property real liveDx: live && selBounds ? live.x - selBounds.x : 0
    readonly property real liveDy: live && selBounds ? live.y - selBounds.y : 0
    readonly property var changedRects: editor.changed ? JSON.parse(editor.changed) : []
    readonly property var notes: {
        const all = editor.annotations ? JSON.parse(editor.annotations) : []
        return all.filter(n => n.status === "open" && n.bounds)
    }
    readonly property var unrendered: editor.unrendered ? JSON.parse(editor.unrendered) : []
    readonly property var overflows: editor.overflows ? JSON.parse(editor.overflows) : []
    JsonModel { id: unrenderedModel }
    JsonModel { id: overflowsModel }
    JsonModel { id: changedModel }
    JsonModel { id: notesModel }
    JsonModel { id: rectsModel }
    onUnrenderedChanged: unrenderedModel.sync(unrendered)
    onOverflowsChanged: overflowsModel.sync(overflows)
    onChangedRectsChanged: changedModel.sync(changedRects)
    onNotesChanged: notesModel.sync(notes)
    onSelRectsChanged: rectsModel.sync(selRects)
    Component.onCompleted: {
        if (editor.canvas !== "") back.source = editor.canvas
        unrenderedModel.sync(unrendered)
        overflowsModel.sync(overflows)
        changedModel.sync(changedRects)
        notesModel.sync(notes)
        rectsModel.sync(selRects)
    }
    readonly property real renderScale: zoom * Screen.devicePixelRatio

    // The angles are counted from where the drag started: the ghost and the commit are the same
    // turn, so what the user sees while dragging is what the device keeps.
    function orbitStart(id, vx, vy) {
        orbit = { id: id, vx: vx, vy: vy, rx: 0, ry: 0, moved: false }
    }
    function orbitMove(vx, vy) {
        if (!orbit) return
        const limit = 3600
        const ry = Math.max(-limit, Math.min(limit, (vx - orbit.vx) * degreesPerPixel))
        const rx = Math.max(-limit, Math.min(limit, -(vy - orbit.vy) * degreesPerPixel))
        orbit = Object.assign({}, orbit, { rx: rx, ry: ry, moved: true })
        if (!orbitTimer.running) orbitTimer.start()
    }
    function orbitEnd() {
        const o = orbit
        orbit = null
        orbitTimer.stop()
        if (o && o.moved) editor.orbitDevice(o.id, o.rx, o.ry)
        renderTimer.restart()
    }
    // the ghost: the canvas redrawn at half scale with the device turned, at most every 40 ms
    Timer {
        id: orbitTimer
        interval: 40
        onTriggered: if (root.orbit) root.editor.renderOrbit(root.renderScale / 2, root.orbit.id, root.orbit.rx, root.orbit.ry)
    }

    function toFrame(vx, vy) { return Qt.point((vx - panX) / zoom, (vy - panY) / zoom) }
    function clampZoom(z) { return Math.max(minZoom, Math.min(maxZoom, z)) }
    function fit() {
        if (fw <= 0 || fh <= 0 || width <= 0 || height <= 0) return
        zoom = clampZoom(Math.min((width - 120) / fw, (height - 120) / fh))
        panX = (width - fw * zoom) / 2
        panY = (height - fh * zoom) / 2
    }
    function zoomAt(factor, vx, vy) {
        const fx = (vx - panX) / zoom
        const fy = (vy - panY) / zoom
        zoom = clampZoom(zoom * factor)
        panX = vx - fx * zoom
        panY = vy - fy * zoom
    }
    function zoomCenter(factor) { zoomAt(factor, width / 2, height / 2) }
    // zoom to a scale (1 = 100%) around the middle of the view
    function zoomTo(z) { zoomAt(z / zoom, width / 2, height / 2); userMoved = true }
    // fit the selected layer (or the whole frame when nothing is selected)
    function fitSelection() {
        const r = selBounds
        if (!r || r.width <= 0 || r.height <= 0) { fit(); return }
        const z = clampZoom(Math.min((width - 160) / r.width, (height - 160) / r.height))
        zoom = z
        panX = (width - r.width * z) / 2 - r.x * z
        panY = (height - r.height * z) / 2 - r.y * z
        userMoved = true
    }

    // The view fits when a frame is opened (and while the user has not moved it, when the window
    // resizes); an edit that changes the frame's size must not move the view under the user.
    property bool userMoved: false
    property string fittedFrame: ""
    onFwChanged: { fitIfNewFrame(); repaintGrid() }
    onFhChanged: { fitIfNewFrame(); repaintGrid() }
    onWidthChanged: { if (!userMoved) fitTimer.restart(); repaintGrid(); repaintRulers() }
    function fitIfNewFrame() {
        if (fw > 0 && fh > 0 && editor.activeFrame !== fittedFrame) {
            fittedFrame = editor.activeFrame
            userMoved = false
            fitTimer.restart()
        }
    }
    Connections { target: root.editor; function onActiveFrameChanged() { root.fitIfNewFrame() } }
    Timer { id: fitTimer; interval: 30; onTriggered: root.fit() }

    // ask the core to redraw once the picture has settled
    // A zoom waits until it settles; edits redraw at most every 90 ms, so a burst of an agent's
    // changes shows as it goes rather than only after it ends.
    Timer { id: renderTimer; interval: 90; onTriggered: root.editor.renderCanvas(root.renderScale) }
    onRenderScaleChanged: renderTimer.restart()
    Connections {
        target: root.editor
        function onRevisionChanged() { if (!renderTimer.running) renderTimer.start() }
        function onActiveFrameChanged() { renderTimer.restart() }
    }

    // ---- gestures to editor calls ----
    function hasMod(mods, flag) { return (mods & flag) !== 0 }
    function skipsSnap(mods) { return hasMod(mods, Qt.ControlModifier) || hasMod(mods, Qt.MetaModifier) }
    // the box moved to (x, y), snapped to its neighbours; the guides to draw come with it
    function snapBox(x, y, w, h, mods) {
        if (!snapping || skipsSnap(mods)) { guides = []; return { x: x, y: y } }
        const s = JSON.parse(editor.snapMove(x, y, w, h, editor.selection))
        guides = s.guides || []
        return { x: s.x, y: s.y }
    }
    // the box `b` dragged by a handle at (sx, sy) (0, 0.5 or 1 on each axis) by (dx, dy)
    function resizeBox(b, sx, sy, dx, dy, mods) {
        const corner = sx !== 0.5 && sy !== 0.5
        const keepRatio = corner && hasMod(mods, Qt.ShiftModifier)
        if (snapping && !skipsSnap(mods) && !keepRatio) {
            const ex = sx === 0 ? b.x + dx : sx === 1 ? b.x + b.width + dx : b.x
            const ey = sy === 0 ? b.y + dy : sy === 1 ? b.y + b.height + dy : b.y
            const s = JSON.parse(editor.snapMove(ex, ey, 0, 0, editor.selection))
            if (sx !== 0.5) dx += s.x - ex
            if (sy !== 0.5) dy += s.y - ey
            guides = (s.guides || []).filter(g => g.axis === "x" ? sx !== 0.5 : sy !== 0.5)
        } else guides = []
        let x = b.x, y = b.y, w = b.width, h = b.height
        if (sx === 0) { x += dx; w -= dx } else if (sx === 1) { w += dx }
        if (sy === 0) { y += dy; h -= dy } else if (sy === 1) { h += dy }
        if (keepRatio) {
            const ratio = b.width / b.height
            if (Math.abs(w - b.width) / b.width > Math.abs(h - b.height) / b.height) h = w / ratio
            else w = h * ratio
            x = sx === 0 ? b.x + b.width - w : b.x
            y = sy === 0 ? b.y + b.height - h : b.y
        }
        if (w < 1) { if (sx === 0) x = b.x + b.width - 1; w = 1 }
        if (h < 1) { if (sy === 0) y = b.y + b.height - 1; h = 1 }
        return { x: x, y: y, width: w, height: h }
    }
    // a shape drawn with a tool: dragged out, or the default size at a click
    function drawShape(kind, a, b, mods, dragged) {
        let x = Math.min(a.x, b.x), y = Math.min(a.y, b.y)
        let w = Math.abs(b.x - a.x), h = Math.abs(b.y - a.y)
        if (!dragged || w < 2 || h < 2) {
            x = a.x; y = a.y
            w = defaultSizes[kind][0]; h = defaultSizes[kind][1]
        } else if (hasMod(mods, Qt.ShiftModifier) && (kind === "rect" || kind === "ellipse")) {
            const s = Math.max(w, h)
            if (b.x < a.x) x = a.x - s
            if (b.y < a.y) y = a.y - s
            w = s; h = s
        }
        const id = editor.createNode(kind, Math.round(x), Math.round(y), Math.round(w), Math.round(h))
        if (id !== "") editor.select(id)
        toolFinished()
    }
    function openContext(fp) {
        const id = editor.hitTest(fp.x, fp.y)
        if (id !== "" && id !== editor.activeFrame && selIds.indexOf(id) < 0) editor.select(id)
        contextMenu.popup()
    }

    clip: true
    focus: true
    Keys.onPressed: e => { if (e.key === Qt.Key_Space && !e.isAutoRepeat) { spaceDown = true; e.accepted = true } }
    Keys.onReleased: e => { if (e.key === Qt.Key_Space && !e.isAutoRepeat) { spaceDown = false; e.accepted = true } }

    Rectangle { anchors.fill: parent; color: Theme.canvasBg }

    // the frame
    Rectangle {
        x: root.panX - 1
        y: root.panY - 1
        width: root.fw * root.zoom + 2
        height: root.fh * root.zoom + 2
        visible: root.fw > 0
        color: "transparent"
        border.width: 1
        border.color: Theme.border
    }
    // The picture is drawn by two images that take turns. A new drawing loads into the one that is
    // hidden and replaces the other only when it is ready, so the canvas is never blank or half
    // loaded while it updates (an image given a new source drops the old one first).
    property int shown: 0
    readonly property var buffers: [bufferA, bufferB]
    readonly property Image front: buffers[shown]
    readonly property Image back: buffers[1 - shown]
    readonly property bool pictureReady: front.status === Image.Ready
    function swapWhenReady(buffer) {
        if (buffer === back && buffer.status === Image.Ready) shown = 1 - shown
    }
    Connections {
        target: root.editor
        function onCanvasChanged() {
            if (root.editor.canvas === "") return
            root.back.source = root.editor.canvas
            root.swapWhenReady(root.back)
        }
    }
    component Buffer: Image {
        x: root.panX
        y: root.panY
        width: root.fw * root.zoom
        height: root.fh * root.zoom
        cache: false
        smooth: true
        mipmap: true
        asynchronous: true
        visible: root.fw > 0 && root.front === this
        onStatusChanged: root.swapWhenReady(this)
    }
    // A frame with no background is transparent. The picture keeps its alpha and the
    // checkerboard shows through, so the canvas does not look like a broken white screen.
    Image {
        x: root.panX
        y: root.panY
        width: root.fw * root.zoom
        height: root.fh * root.zoom
        visible: root.fw > 0
        source: root.editor.checkerboard
        fillMode: Image.Tile
        sourceSize.width: 32
        sourceSize.height: 32
        cache: true
    }
    Buffer { id: bufferA }
    Buffer { id: bufferB }

    // images the renderer cannot draw (no file, unreadable file) are marked here with the reason
    Repeater {
        model: unrenderedModel
        delegate: Item {
            id: ph
            visible: root.overlays
            required property string json
            readonly property var modelData: JSON.parse(json)
            x: root.panX + modelData.x * root.zoom
            y: root.panY + modelData.y * root.zoom
            width: modelData.width * root.zoom
            height: modelData.height * root.zoom
            Rectangle {
                anchors.fill: parent
                color: "transparent"
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.25)
            }
            Text {
                anchors.fill: parent
                anchors.margins: 4
                text: "image: " + ph.modelData.reason
                color: Theme.muted
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
            }
        }
    }

    // text that runs past its box: the box is outlined and says by how much
    Repeater {
        model: overflowsModel
        delegate: Item {
            id: ov
            visible: root.overlays
            required property string json
            readonly property var modelData: JSON.parse(json)
            x: root.panX + modelData.x * root.zoom
            y: root.panY + modelData.y * root.zoom
            width: modelData.width * root.zoom
            height: modelData.height * root.zoom
            Rectangle {
                anchors.fill: parent
                color: "transparent"
                border.width: 1
                border.color: Theme.danger
            }
            Text {
                anchors.top: parent.bottom
                anchors.left: parent.left
                anchors.topMargin: 2
                text: "text overflows: needs " + ov.modelData.needed + ", box is " + ov.modelData.available
                color: Theme.danger
                font.pixelSize: 11
            }
        }
    }

    // what agents changed
    Repeater {
        model: changedModel
        delegate: Item {
            required property string json
            readonly property var modelData: JSON.parse(json)
            visible: root.overlays
            x: root.panX + modelData.x * root.zoom
            y: root.panY + modelData.y * root.zoom
            width: modelData.width * root.zoom
            height: modelData.height * root.zoom
            Rectangle { anchors.fill: parent; color: Qt.rgba(0.35, 0.69, 1, 0.08); border.width: 2; border.color: Theme.agent }
            Rectangle {
                y: -16
                width: tag.implicitWidth + 8
                height: 16
                color: Theme.agent
                radius: 2
                Text { id: tag; anchors.centerIn: parent; text: "agent"; color: "#06192b"; font.pixelSize: 10; font.bold: true }
            }
        }
    }

    // pinned notes
    Repeater {
        model: notesModel
        delegate: Item {
            id: pin
            visible: root.overlays
            required property string json
            readonly property var modelData: JSON.parse(json)
            x: root.panX + modelData.bounds.x * root.zoom
            y: root.panY + modelData.bounds.y * root.zoom
            width: modelData.bounds.width * root.zoom
            height: modelData.bounds.height * root.zoom
            Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 0.62, 0.26, 0.7) }
            Rectangle {
                x: -9
                y: -9
                width: 18
                height: 18
                radius: 9
                color: Theme.pin
                border.width: 2
                border.color: Theme.canvasBg
                Text { anchors.centerIn: parent; text: pin.modelData.id; color: "#2a1500"; font.pixelSize: 10; font.bold: true }
                TapHandler { onTapped: root.noteClicked(pin.modelData.id) }
            }
        }
    }

    // the pixel grid: one line per document pixel once the zoom shows them as cells
    Canvas {
        id: pixelGridLayer
        anchors.fill: parent
        visible: root.pixelGrid && root.zoom >= 8 && root.fw > 0
        onPaint: {
            const ctx = getContext("2d")
            ctx.clearRect(0, 0, width, height)
            const z = root.zoom
            const x0 = Math.max(0, Math.floor(-root.panX / z)), x1 = Math.min(root.fw, Math.ceil((width - root.panX) / z))
            const y0 = Math.max(0, Math.floor(-root.panY / z)), y1 = Math.min(root.fh, Math.ceil((height - root.panY) / z))
            ctx.strokeStyle = "rgba(128,128,128,0.35)"
            ctx.lineWidth = 1
            ctx.beginPath()
            for (let x = x0; x <= x1; ++x) { const p = Math.round(root.panX + x * z) + 0.5; ctx.moveTo(p, root.panY + y0 * z); ctx.lineTo(p, root.panY + y1 * z) }
            for (let y = y0; y <= y1; ++y) { const p = Math.round(root.panY + y * z) + 0.5; ctx.moveTo(root.panX + x0 * z, p); ctx.lineTo(root.panX + x1 * z, p) }
            ctx.stroke()
        }
    }
    function repaintGrid() { if (pixelGridLayer.visible) pixelGridLayer.requestPaint() }
    function repaintRulers() { if (rulers) { rulerTop.requestPaint(); rulerLeft.requestPaint() } }
    onPanXChanged: { repaintGrid(); repaintRulers() }
    onPanYChanged: { repaintGrid(); repaintRulers() }
    onZoomChanged: { repaintGrid(); repaintRulers() }

    onHeightChanged: { repaintGrid(); repaintRulers() }
    onRulersChanged: repaintRulers()

    Connections { target: pixelGridLayer; function onVisibleChanged() { root.repaintGrid() } }

    // selection: an outline per layer when several are selected, and the box around them all
    Repeater {
        model: rectsModel
        delegate: Rectangle {
            required property string json
            readonly property var r: JSON.parse(json)
            visible: root.overlays && root.multi && root.tool === "select"
            x: root.panX + (r.x + root.liveDx) * root.zoom
            y: root.panY + (r.y + root.liveDy) * root.zoom
            width: r.w * root.zoom
            height: r.h * root.zoom
            color: "transparent"
            border.width: 1
            border.color: Qt.rgba(0.95, 0.79, 0.51, 0.7)
        }
    }
    Item {
        id: selection
        readonly property var r: root.live ? root.live : root.selBounds
        visible: root.overlays && !!r && root.tool === "select"
        x: r ? root.panX + r.x * root.zoom : 0
        y: r ? root.panY + r.y * root.zoom : 0
        width: r ? r.width * root.zoom : 0
        height: r ? r.height * root.zoom : 0
        Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1.5; border.color: Theme.accent }
        Rectangle {
            y: parent.height + 4
            width: dims.implicitWidth + 10
            height: 16
            radius: 3
            color: Theme.accent
            visible: !!selection.r
            Text {
                id: dims
                anchors.centerIn: parent
                text: selection.r ? Math.round(selection.r.width) + " × " + Math.round(selection.r.height) : ""
                color: Theme.textOnAccent
                font.pixelSize: 10
                font.bold: true
            }
        }

        component Handle: Rectangle {
            id: h
            required property real sx
            required property real sy
            property point pressView
            property var start: null
            width: 9
            height: 9
            x: parent.width * sx - width / 2
            y: parent.height * sy - height / 2
            color: "white"
            border.width: 1.5
            border.color: Theme.accent
            visible: root.canResize
            MouseArea {
                anchors.fill: parent
                anchors.margins: -5
                cursorShape: (h.sx === h.sy || (h.sx === 0 && h.sy === 1) || (h.sx === 1 && h.sy === 0)) ? ((h.sx === h.sy) ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor) : (h.sx === 0.5 ? Qt.SizeVerCursor : Qt.SizeHorCursor)
                onPressed: m => {
                    const p = mapToItem(root, m.x, m.y)
                    h.pressView = Qt.point(p.x, p.y)
                    h.start = { b: root.selBounds, n: root.node }
                }
                onPositionChanged: m => {
                    if (!h.start) return
                    const p = mapToItem(root, m.x, m.y)
                    const dx = (p.x - h.pressView.x) / root.zoom
                    const dy = (p.y - h.pressView.y) / root.zoom
                    root.live = root.resizeBox(h.start.b, h.sx, h.sy, dx, dy, m.modifiers)
                }
                onReleased: {
                    if (h.start && root.live) {
                        const b = h.start.b, n = h.start.n, l = root.live
                        root.editor.resizeNode(n.id, n.x + (l.x - b.x), n.y + (l.y - b.y), l.width, l.height)
                    }
                    root.live = null
                    root.guides = []
                    h.start = null
                }
            }
        }
        Handle { sx: 0; sy: 0 }
        Handle { sx: 0.5; sy: 0 }
        Handle { sx: 1; sy: 0 }
        Handle { sx: 1; sy: 0.5 }
        Handle { sx: 1; sy: 1 }
        Handle { sx: 0.5; sy: 1 }
        Handle { sx: 0; sy: 1 }
        Handle { sx: 0; sy: 0.5 }
        // turns a 3D device: drag it (or Alt-drag the device); horizontal turns, vertical tilts
        Rectangle {
            id: rotateHandle
            visible: !!root.node && root.node.kind === "device3d"
            width: 24
            height: 24
            radius: 12
            x: parent.width / 2 - width / 2
            y: -height - 10
            color: rotateArea.pressed ? Theme.accentStrong : Theme.accent
            Accessible.name: "Turn the device"
            Accessible.role: Accessible.Button
            Icon { anchors.centerIn: parent; name: "rotate-3d"; size: 16; color: Theme.textOnAccent }
            MouseArea {
                id: rotateArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.SizeAllCursor
                onPressed: m => { const p = mapToItem(root, m.x, m.y); root.orbitStart(root.node.id, p.x, p.y) }
                onPositionChanged: m => { if (pressed) { const p = mapToItem(root, m.x, m.y); root.orbitMove(p.x, p.y) } }
                onReleased: root.orbitEnd()
                onCanceled: root.orbitEnd()
                ToolTip.visible: containsMouse && !pressed
                ToolTip.delay: 600
                ToolTip.text: "Turn the device: drag sideways to turn, up or down to tilt (or Alt-drag it)"
            }
        }
    }

    // snapping guides
    Repeater {
        model: root.overlays ? root.guides : []
        delegate: Rectangle {
            required property var modelData
            readonly property bool vertical: modelData.axis === "x"
            color: root.guideColor
            x: root.panX + (vertical ? modelData.pos : modelData.from) * root.zoom
            y: root.panY + (vertical ? modelData.from : modelData.pos) * root.zoom
            width: vertical ? 1 : (modelData.to - modelData.from) * root.zoom
            height: vertical ? (modelData.to - modelData.from) * root.zoom : 1
        }
    }

    // the rubber band while drawing a new shape, selecting by box, or marking a pin region
    Rectangle {
        id: band
        visible: false
        color: Qt.rgba(1, 1, 1, 0.06)
        border.width: 1
        border.color: root.tool === "pin" ? Theme.pin : Theme.accent
    }

    Menu {
        id: contextMenu
        MenuItem { text: "Duplicate"; enabled: root.selIds.length > 0; onTriggered: root.editor.duplicateSelected() }
        MenuItem { text: "Group"; enabled: root.editor.canGroup === true; onTriggered: root.editor.groupSelected() }
        MenuItem { text: "Ungroup"; enabled: root.editor.canUngroup === true; onTriggered: root.editor.ungroupSelected() }
        MenuSeparator {}
        MenuItem { text: "Copy"; enabled: root.selIds.length > 0; onTriggered: root.editor.copySelected() }
        MenuItem { text: "Paste"; enabled: root.editor.canPaste === true; onTriggered: root.editor.pasteClipboard() }
        MenuSeparator {}
        MenuItem { text: "Bring forward"; enabled: root.selIds.length > 0; onTriggered: root.editor.bringSelected(1) }
        MenuItem { text: "Send backward"; enabled: root.selIds.length > 0; onTriggered: root.editor.bringSelected(-1) }
        MenuItem { text: "Bring to front"; enabled: root.selIds.length > 0; onTriggered: root.editor.bringSelected(1000) }
        MenuItem { text: "Send to back"; enabled: root.selIds.length > 0; onTriggered: root.editor.bringSelected(-1000) }
        MenuSeparator {}
        MenuItem { text: "Delete"; enabled: root.selIds.length > 0; onTriggered: root.editor.deleteSelected() }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true
        cursorShape: root.spaceDown || root.tool === "hand" ? (pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor) : (root.tool === "select" ? Qt.ArrowCursor : Qt.CrossCursor)
        z: -1

        property point startView
        property point startFrame
        property point panStart
        // what the press began: pan | move | marquee | draw | pin
        property string mode: ""
        property bool moved: false
        property bool additive: false
        property bool altDuplicate: false
        // a press on a selected layer decides on release: Shift removes it, otherwise it alone stays
        property string releaseId: ""
        property string pressedId: ""
        property var dragIds: []
        property var dragNode: null
        property var dragBounds: null

        function beginMove(id, mods) {
            additive = root.hasMod(mods, Qt.ShiftModifier)
            const inSelection = root.selIds.indexOf(id) >= 0
            if (additive && !inSelection) root.editor.toggleSelect(id)
            else if (!additive && !inSelection) root.editor.select(id)
            else releaseId = id
            altDuplicate = root.hasMod(mods, Qt.AltModifier)
            dragIds = root.selIds
            dragNode = root.node
            dragBounds = root.selBounds
            mode = "move"
        }
        function dragMove(m) {
            if (!dragBounds) return
            if (altDuplicate) {
                altDuplicate = false
                root.editor.duplicateSelected()
                dragIds = root.selIds
                dragNode = root.node
                dragBounds = root.selBounds
            }
            let dx = (m.x - startView.x) / root.zoom
            let dy = (m.y - startView.y) / root.zoom
            if (root.hasMod(m.modifiers, Qt.ShiftModifier) && additive) { if (Math.abs(dx) > Math.abs(dy)) dy = 0; else dx = 0 }
            const b = dragBounds
            const s = root.snapBox(b.x + dx, b.y + dy, b.width, b.height, m.modifiers)
            root.live = { x: s.x, y: s.y, width: b.width, height: b.height }
        }
        function dragBand(m, mods) {
            band.visible = moved
            band.x = Math.min(startView.x, m.x)
            band.y = Math.min(startView.y, m.y)
            band.width = Math.abs(m.x - startView.x)
            band.height = Math.abs(m.y - startView.y)
        }
        function finishMove() {
            if (moved && root.live && dragBounds) {
                const dx = root.live.x - dragBounds.x, dy = root.live.y - dragBounds.y
                if (dragIds.length > 1) root.editor.moveSelected(dx, dy)
                else if (dragNode) root.editor.moveNode(dragNode.id, dragNode.x + dx, dragNode.y + dy)
            } else if (!moved && releaseId !== "") {
                if (additive) root.editor.toggleSelect(releaseId)
                else root.editor.select(releaseId)
            }
        }
        function finishMarquee(end) {
            if (moved) {
                root.editor.selectInRect(Math.min(startFrame.x, end.x), Math.min(startFrame.y, end.y),
                                         Math.abs(end.x - startFrame.x), Math.abs(end.y - startFrame.y), additive)
            } else if (!additive) {
                root.editor.select(pressedId)
            }
        }
        function finishPin(end) {
            const id = root.editor.hitTest(startFrame.x, startFrame.y)
            const w = Math.abs(end.x - startFrame.x), h = Math.abs(end.y - startFrame.y)
            if (moved && w > 4 && h > 4)
                root.requestPin("", Math.min(startFrame.x, end.x), Math.min(startFrame.y, end.y), w, h)
            else
                root.requestPin(id, 0, 0, 0, 0)
            root.toolFinished()
        }

        onPressed: m => {
            root.forceActiveFocus()
            startView = Qt.point(m.x, m.y)
            startFrame = root.toFrame(m.x, m.y)
            moved = false
            additive = false
            releaseId = ""
            pressedId = ""
            mode = ""
            if (m.button === Qt.RightButton) { root.openContext(startFrame); return }
            if (m.button === Qt.MiddleButton || root.spaceDown || root.tool === "hand") {
                mode = "pan"
                panStart = Qt.point(root.panX, root.panY)
                return
            }
            if (root.tool === "select") {
                if (m.modifiers & Qt.AltModifier) {
                    // a drag with the modifier turns a 3D device: the one under the pointer, or
                    // the one that is selected
                    const under = root.editor.deviceAt(startFrame.x, startFrame.y)
                    const selected = root.node && root.node.kind === "device3d" ? root.node.id : ""
                    const device = under !== "" ? under : selected
                    if (device !== "") {
                        if (device !== selected) root.editor.select(device)
                        root.orbitStart(device, m.x, m.y)
                        mode = "orbit"
                        return
                    }
                }
                const id = root.editor.hitTest(startFrame.x, startFrame.y)
                if (id === "" || id === root.editor.activeFrame) {
                    mode = "marquee"
                    additive = root.hasMod(m.modifiers, Qt.ShiftModifier)
                    pressedId = id
                } else beginMove(id, m.modifiers)
            } else if (root.tool === "pin") mode = "pin"
            else mode = "draw"
        }
        onPositionChanged: m => {
            root.pointer = root.toFrame(m.x, m.y)
            if (!pressed) return
            if (mode === "orbit") { root.orbitMove(m.x, m.y); return }
            if (mode === "pan") {
                root.panX = panStart.x + (m.x - startView.x)
                root.panY = panStart.y + (m.y - startView.y)
                root.userMoved = true
                return
            }
            if (Math.abs(m.x - startView.x) + Math.abs(m.y - startView.y) > 3) moved = true
            if (!moved) return
            if (mode === "move") dragMove(m)
            else if (mode === "marquee" || mode === "draw" || mode === "pin") dragBand(m)
        }
        onReleased: m => {
            band.visible = false
            const end = root.toFrame(m.x, m.y)
            if (mode === "orbit") root.orbitEnd()
            else if (mode === "move") finishMove()
            else if (mode === "marquee") finishMarquee(end)
            else if (mode === "pin") finishPin(end)
            else if (mode === "draw") root.drawShape(root.tool, startFrame, end, m.modifiers, moved)
            root.live = null
            root.guides = []
            dragNode = null
            dragBounds = null
            mode = ""
        }
    }

    WheelHandler {
        acceptedModifiers: Qt.NoModifier
        onWheel: e => {
            root.panX += e.pixelDelta.x !== 0 ? e.pixelDelta.x : e.angleDelta.x / 4
            root.panY += e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y / 4
            root.userMoved = true
        }
    }
    // ⌘ (or Ctrl) and ⌥ with the scroll wheel or two fingers zoom around the pointer
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier | Qt.MetaModifier | Qt.AltModifier
        onWheel: e => {
            const d = e.pixelDelta.y !== 0 ? e.pixelDelta.y * 2 : e.angleDelta.y
            root.zoomAt(Math.pow(1.0015, d), point.position.x, point.position.y)
            root.userMoved = true
        }
    }
    PinchHandler {
        id: pinch
        target: null
        property real lastScale: 1
        onActiveChanged: lastScale = 1
        onScaleChanged: {
            if (!active) return
            root.zoomAt(scale / lastScale, centroid.position.x, centroid.position.y)
            lastScale = scale
        }
    }
    // Files dropped here are not placed on the design: they go to the agent's memory (the media
    // folder) for it to use when it needs them.
    DropArea {
        anchors.fill: parent
        // a picture or a video dropped on a 3D device becomes its screen; the rest goes to memory
        onDropped: drop => {
            const at = root.toFrame(drop.x, drop.y)
            let device = root.editor.deviceAt(at.x, at.y)
            for (const url of drop.urls) {
                if (device !== "") { root.editor.dropOnDevice(url.toString(), device); device = "" }
                else root.editor.importToMemory(url.toString())
            }
        }
    }

    // rulers along the top and left edges, in document pixels, with the pointer and the selection
    component Ruler: Canvas {
        id: ruler
        required property bool horizontal
        visible: root.rulers
        x: horizontal ? root.rulerSize : 0
        y: horizontal ? 0 : root.rulerSize
        width: horizontal ? root.width - root.rulerSize : root.rulerSize
        height: horizontal ? root.rulerSize : root.height - root.rulerSize
        onPaint: {
            const ctx = getContext("2d")
            ctx.clearRect(0, 0, width, height)
            ctx.fillStyle = Theme.panel
            ctx.fillRect(0, 0, width, height)
            const len = horizontal ? width : height
            const off = (horizontal ? root.panX : root.panY) - root.rulerSize
            const major = root.rulerStep()
            const unit = major >= 5 ? major / 5 : (major === 2 ? 1 : major)
            ctx.strokeStyle = Theme.muted
            ctx.fillStyle = Theme.muted
            ctx.font = "9px sans-serif"
            ctx.lineWidth = 1
            ctx.beginPath()
            const first = Math.floor(-off / root.zoom / unit) * unit
            for (let v = first; v * root.zoom + off <= len; v += unit) {
                const p = Math.round(off + v * root.zoom) + 0.5
                const isMajor = Math.abs(v % major) < 1e-9
                const t = isMajor ? 12 : 5
                if (horizontal) { ctx.moveTo(p, root.rulerSize - t); ctx.lineTo(p, root.rulerSize) }
                else { ctx.moveTo(root.rulerSize - t, p); ctx.lineTo(root.rulerSize, p) }
            }
            ctx.stroke()
            for (let v = first; v * root.zoom + off <= len; v += unit) {
                if (Math.abs(v % major) >= 1e-9) continue
                const p = off + v * root.zoom
                if (horizontal) ctx.fillText(String(v), p + 3, 9)
                else { ctx.save(); ctx.translate(9, p - 3); ctx.rotate(-Math.PI / 2); ctx.fillText(String(v), 0, 0); ctx.restore() }
            }
            ctx.strokeStyle = Theme.border
            ctx.beginPath()
            if (horizontal) { ctx.moveTo(0, height - 0.5); ctx.lineTo(width, height - 0.5) }
            else { ctx.moveTo(width - 0.5, 0); ctx.lineTo(width - 0.5, height) }
            ctx.stroke()
        }
    }
    // the document pixels between major ticks: at least 64 screen pixels apart
    function rulerStep() {
        for (const s of [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000, 50000])
            if (s * zoom >= 64) return s
        return 100000
    }
    Ruler { id: rulerTop; horizontal: true }
    Ruler { id: rulerLeft; horizontal: false }
    Rectangle { visible: root.rulers; width: root.rulerSize; height: root.rulerSize; color: Theme.panel }
    // the selection's extent and the pointer on the rulers
    Rectangle {
        visible: root.rulers && root.overlays && !!root.selBounds
        x: root.selBounds ? root.panX + root.selBounds.x * root.zoom : 0
        y: 0
        width: root.selBounds ? root.selBounds.width * root.zoom : 0
        height: root.rulerSize
        color: Qt.rgba(0.95, 0.79, 0.51, 0.25)
    }
    Rectangle {
        visible: root.rulers && root.overlays && !!root.selBounds
        x: 0
        y: root.selBounds ? root.panY + root.selBounds.y * root.zoom : 0
        width: root.rulerSize
        height: root.selBounds ? root.selBounds.height * root.zoom : 0
        color: Qt.rgba(0.95, 0.79, 0.51, 0.25)
    }
    Rectangle {
        visible: root.rulers && mouse.containsMouse
        x: root.panX + root.pointer.x * root.zoom
        width: 1
        height: root.rulerSize
        color: Theme.accent
    }
    Rectangle {
        visible: root.rulers && mouse.containsMouse
        y: root.panY + root.pointer.y * root.zoom
        width: root.rulerSize
        height: 1
        color: Theme.accent
    }
}
