import QtQuick
import QtQuick.Controls

// The active frame, rendered by the Rust core, with the editing overlays on top: selection and
// resize handles, the outlines of what an agent changed, the user's pinned notes, and the drag
// ghost. Pan and zoom move the picture without asking the core to redraw; the core redraws when
// the document or the zoom settles.
Item {
    id: root
    required property var editor
    property string tool: "select"
    property real zoom: 0.25
    property real panX: 40
    property real panY: 40
    property bool spaceDown: false
    // a rectangle in frame coordinates shown while dragging or resizing
    property var live: null

    signal requestPin(string nodeId, real x, real y, real w, real h)
    signal noteClicked(int noteId)
    signal toolFinished()

    readonly property var frameSize: editor.frameSize && editor.frameSize !== "null" ? JSON.parse(editor.frameSize) : null
    readonly property real fw: frameSize ? frameSize.width : 0
    readonly property real fh: frameSize ? frameSize.height : 0
    readonly property var selBounds: editor.selectionBounds && editor.selectionBounds !== "null" ? JSON.parse(editor.selectionBounds) : null
    readonly property var node: editor.inspector && editor.inspector !== "null" ? JSON.parse(editor.inspector) : null
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
    onUnrenderedChanged: unrenderedModel.sync(unrendered)
    onOverflowsChanged: overflowsModel.sync(overflows)
    onChangedRectsChanged: changedModel.sync(changedRects)
    onNotesChanged: notesModel.sync(notes)
    Component.onCompleted: {
        if (editor.canvas !== "") back.source = editor.canvas
        unrenderedModel.sync(unrendered)
        overflowsModel.sync(overflows)
        changedModel.sync(changedRects)
        notesModel.sync(notes)
    }
    readonly property real renderScale: zoom * Screen.devicePixelRatio

    function toFrame(vx, vy) { return Qt.point((vx - panX) / zoom, (vy - panY) / zoom) }
    function fit() {
        if (fw <= 0 || fh <= 0 || width <= 0 || height <= 0) return
        const z = Math.min((width - 120) / fw, (height - 120) / fh)
        zoom = Math.max(0.02, Math.min(8, z))
        panX = (width - fw * zoom) / 2
        panY = (height - fh * zoom) / 2
    }
    function zoomAt(factor, vx, vy) {
        const fx = (vx - panX) / zoom
        const fy = (vy - panY) / zoom
        zoom = Math.max(0.02, Math.min(8, zoom * factor))
        panX = vx - fx * zoom
        panY = vy - fy * zoom
    }
    function zoomCenter(factor) { zoomAt(factor, width / 2, height / 2) }

    // The view fits when a frame is opened (and while the user has not moved it, when the window
    // resizes); an edit that changes the frame's size must not move the view under the user.
    property bool userMoved: false
    property string fittedFrame: ""
    onFwChanged: fitIfNewFrame()
    onFhChanged: fitIfNewFrame()
    onWidthChanged: if (!userMoved) fitTimer.restart()
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
    Buffer { id: bufferA }
    Buffer { id: bufferB }

    // images the renderer cannot draw (no file, unreadable file) are marked here with the reason
    Repeater {
        model: unrenderedModel
        delegate: Item {
            id: ph
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

    // selection with resize handles
    Item {
        id: selection
        readonly property var r: root.live ? root.live : root.selBounds
        visible: !!r && root.tool === "select"
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
            visible: !!root.node && root.node.kind !== "group"
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
                    const b = h.start.b
                    let x = b.x, y = b.y, w = b.width, hh = b.height
                    if (h.sx === 0) { x += dx; w -= dx } else if (h.sx === 1) { w += dx }
                    if (h.sy === 0) { y += dy; hh -= dy } else if (h.sy === 1) { hh += dy }
                    if (w < 1) { if (h.sx === 0) x = b.x + b.width - 1; w = 1 }
                    if (hh < 1) { if (h.sy === 0) y = b.y + b.height - 1; hh = 1 }
                    root.live = { x: x, y: y, width: w, height: hh }
                }
                onReleased: {
                    if (h.start && root.live) {
                        const b = h.start.b, n = h.start.n, l = root.live
                        root.editor.resizeNode(n.id, n.x + (l.x - b.x), n.y + (l.y - b.y), l.width, l.height)
                    }
                    root.live = null
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
    }

    // the rubber band while drawing a new shape or a pin region
    Rectangle {
        id: band
        visible: false
        color: Qt.rgba(1, 1, 1, 0.06)
        border.width: 1
        border.color: root.tool === "pin" ? Theme.pin : Theme.accent
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        hoverEnabled: true
        cursorShape: root.spaceDown ? Qt.OpenHandCursor : (root.tool !== "select" ? Qt.CrossCursor : Qt.ArrowCursor)
        z: -1

        property point startView
        property point startFrame
        property point panStart
        property bool panning: false
        property bool moved: false
        property var dragNode: null
        property var dragBounds: null

        onPressed: m => {
            root.forceActiveFocus()
            startView = Qt.point(m.x, m.y)
            startFrame = root.toFrame(m.x, m.y)
            moved = false
            panning = m.button === Qt.MiddleButton || root.spaceDown
            if (panning) { panStart = Qt.point(root.panX, root.panY); return }
            if (root.tool === "select") {
                const id = root.editor.hitTest(startFrame.x, startFrame.y)
                if (id === "") { root.editor.select(""); dragNode = null; return }
                if (!root.node || root.node.id !== id) root.editor.select(id)
                dragNode = root.node
                dragBounds = root.selBounds
            }
        }
        onPositionChanged: m => {
            if (!pressed) return
            if (panning) {
                root.panX = panStart.x + (m.x - startView.x)
                root.panY = panStart.y + (m.y - startView.y)
                root.userMoved = true
                return
            }
            const dx = (m.x - startView.x) / root.zoom
            const dy = (m.y - startView.y) / root.zoom
            if (Math.abs(m.x - startView.x) + Math.abs(m.y - startView.y) > 3) moved = true
            if (root.tool === "select") {
                if (dragNode && dragBounds && moved)
                    root.live = { x: dragBounds.x + dx, y: dragBounds.y + dy, width: dragBounds.width, height: dragBounds.height }
            } else {
                band.visible = moved
                band.x = Math.min(startView.x, m.x)
                band.y = Math.min(startView.y, m.y)
                band.width = Math.abs(m.x - startView.x)
                band.height = Math.abs(m.y - startView.y)
            }
        }
        onReleased: m => {
            band.visible = false
            if (panning) { panning = false; return }
            const end = root.toFrame(m.x, m.y)
            if (root.tool === "select") {
                if (dragNode && root.live && moved)
                    root.editor.moveNode(dragNode.id, dragNode.x + (root.live.x - dragBounds.x), dragNode.y + (root.live.y - dragBounds.y))
                root.live = null
                dragNode = null
            } else if (root.tool === "pin") {
                const id = root.editor.hitTest(startFrame.x, startFrame.y)
                const w = Math.abs(end.x - startFrame.x), h = Math.abs(end.y - startFrame.y)
                if (moved && w > 4 && h > 4)
                    root.requestPin("", Math.min(startFrame.x, end.x), Math.min(startFrame.y, end.y), w, h)
                else
                    root.requestPin(id, 0, 0, 0, 0)
                root.toolFinished()
            } else {
                const w = Math.abs(end.x - startFrame.x), h = Math.abs(end.y - startFrame.y)
                const x = Math.min(startFrame.x, end.x), y = Math.min(startFrame.y, end.y)
                if (moved && w > 4 && h > 4) root.editor.createNode(root.tool, x, y, w, h)
                else root.editor.createNode(root.tool, startFrame.x, startFrame.y, root.tool === "text" ? 400 : 200, root.tool === "text" ? 120 : 200)
                root.toolFinished()
            }
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
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier | Qt.MetaModifier
        onWheel: e => root.zoomAt(Math.pow(1.0015, e.angleDelta.y), point.position.x, point.position.y)
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
    // dropping image files: onto an image (a device's screen) swaps its picture, elsewhere adds an image
    DropArea {
        anchors.fill: parent
        onDropped: drop => {
            const p = root.toFrame(drop.x, drop.y)
            for (const url of drop.urls) root.editor.dropImage(url.toString(), p.x, p.y)
        }
    }
}
