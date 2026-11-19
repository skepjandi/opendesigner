import QtQuick

// Children side by side or stacked, sized by weights that add up to 1, with a handle between
// each pair. A drag moves the boundary live; the new sizes are kept when the button is released.
Item {
    id: split
    required property var host
    required property int nodeId
    required property int windowId
    readonly property var spec: host.node(nodeId)
    readonly property bool row: spec !== null && spec.dir === "row"
    // the children's ids as a string, so a change of sizes does not rebuild them
    readonly property string childKey: spec && spec.children ? spec.children.map(c => c.id).join(",") : ""
    readonly property var childIds: childKey === "" ? [] : childKey.split(",").map(Number)
    readonly property int handle: 6
    property var live: null
    readonly property var weights: live ? live : (spec ? spec.weights : [])
    readonly property real avail: Math.max(1, (row ? width : height) - handle * Math.max(0, childIds.length - 1))
    readonly property real minFraction: Math.min(0.4, 96 / avail)

    function offsetOf(i) {
        let s = 0
        for (let k = 0; k < i; ++k) s += weights[k]
        return s * avail + i * handle
    }
    function sizeOf(i) { return (weights[i] || 0) * avail }

    // each child is a DockNode, loaded by url: the two files refer to each other, and a type
    // cycle is not allowed
    Repeater {
        model: split.childIds
        delegate: Loader {
            id: child
            required property int modelData
            required property int index
            x: split.row ? split.offsetOf(index) : 0
            y: split.row ? 0 : split.offsetOf(index)
            width: split.row ? split.sizeOf(index) : split.width
            height: split.row ? split.height : split.sizeOf(index)
            Component.onCompleted: setSource(Qt.resolvedUrl("DockNode.qml"), {
                host: split.host, nodeId: modelData, windowId: split.windowId
            })
            onLoaded: { item.width = Qt.binding(() => child.width); item.height = Qt.binding(() => child.height) }
        }
    }

    Repeater {
        model: Math.max(0, split.childIds.length - 1)
        delegate: Rectangle {
            id: grip
            required property int index
            x: split.row ? split.offsetOf(index) + split.sizeOf(index) : 0
            y: split.row ? 0 : split.offsetOf(index) + split.sizeOf(index)
            width: split.row ? split.handle : split.width
            height: split.row ? split.height : split.handle
            color: Theme.canvasBg
            Rectangle {
                anchors.centerIn: parent
                width: split.row ? 1 : parent.width
                height: split.row ? parent.height : 1
                color: gripArea.containsMouse || gripArea.pressed ? Theme.accent : Theme.border
            }
            MouseArea {
                id: gripArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: split.row ? Qt.SplitHCursor : Qt.SplitVCursor
                property var start: null
                property real startPair: 0
                property real startOffset: 0
                onPressed: mouse => {
                    start = split.weights.slice()
                    startPair = start[grip.index] + start[grip.index + 1]
                    startOffset = split.offsetOf(grip.index)
                }
                onPositionChanged: mouse => {
                    if (!pressed || !start) return
                    const p = split.mapFromItem(gripArea, mouse.x, mouse.y)
                    const along = split.row ? p.x : p.y
                    const first = (along - split.handle / 2 - startOffset) / split.avail
                    const lo = split.minFraction, hi = startPair - split.minFraction
                    const w = start.slice()
                    w[grip.index] = Math.max(lo, Math.min(hi, first))
                    w[grip.index + 1] = startPair - w[grip.index]
                    split.live = w
                }
                onReleased: {
                    if (split.live) split.host.commitWeights(split.nodeId, split.live)
                    split.live = null
                    start = null
                }
            }
        }
    }
}
