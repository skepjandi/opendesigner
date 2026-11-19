import QtQuick

// One node of the layout tree: a split of two or more nodes, or a group of tabs.
Item {
    id: node
    required property var host
    required property int nodeId
    /// 0 for the main window, else the id of the floating window this tree is in.
    required property int windowId
    readonly property var spec: host.node(nodeId)

    Loader {
        anchors.fill: parent
        sourceComponent: node.spec === null ? null : (node.spec.kind === "split" ? splitC : groupC)
    }
    Component { id: splitC; DockSplit { host: node.host; nodeId: node.nodeId; windowId: node.windowId } }
    Component { id: groupC; DockGroup { host: node.host; nodeId: node.nodeId; windowId: node.windowId } }
}
