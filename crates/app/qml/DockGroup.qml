import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml

// A set of panels shown as tabs, one at a time. Tabs can be dragged onto another group (to join
// it or to split next to it) or off the window (to get a window of their own); the menu does the
// same without dragging.
Rectangle {
    id: group
    required property var host
    required property int nodeId
    required property int windowId
    readonly property var spec: host.node(nodeId)
    readonly property string panelKey: spec && spec.panels ? spec.panels.join(",") : ""
    readonly property var panelIds: panelKey === "" ? [] : panelKey.split(",")
    readonly property string active: spec && spec.active ? spec.active : ""
    color: Theme.panel
    clip: true

    Component.onCompleted: host.registerGroup(group)
    Component.onDestruction: host.unregisterGroup(group)

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            color: Theme.panelRaised
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }

            Flickable {
                id: strip
                anchors.left: parent.left
                anchors.right: menuButton.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                contentWidth: tabs.width
                contentHeight: height
                clip: true
                interactive: contentWidth > width
                boundsBehavior: Flickable.StopAtBounds

                Row {
                    id: tabs
                    height: parent.height
                    Repeater {
                        model: group.panelIds
                        delegate: Rectangle {
                            id: tab
                            required property string modelData
                            readonly property bool current: modelData === group.active
                            height: strip.height
                            width: tabLabel.implicitWidth + 22
                            color: current ? Theme.panel : "transparent"
                            opacity: group.host.dragging === modelData ? 0.5 : 1
                            Label {
                                id: tabLabel
                                anchors.centerIn: parent
                                text: group.host.title(tab.modelData)
                                color: tab.current ? Theme.accent : Theme.muted
                                font.pixelSize: Theme.fontSize
                            }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: tab.current ? Theme.accent : "transparent" }
                            TapHandler { acceptedButtons: Qt.LeftButton; onTapped: group.host.activate(group.nodeId, tab.modelData) }
                            TapHandler { acceptedButtons: Qt.RightButton; onTapped: { menu.panel = tab.modelData; menu.popup() } }
                            DragHandler {
                                target: null
                                acceptedButtons: Qt.LeftButton
                                cursorShape: Qt.ClosedHandCursor
                                onActiveChanged: {
                                    if (active) group.host.dragStart(tab.modelData)
                                    else group.host.dragEnd(tab.mapToGlobal(centroid.position))
                                }
                                onCentroidChanged: if (active) group.host.dragMove(tab.mapToGlobal(centroid.position))
                            }
                            ToolTip.visible: hovh.hovered
                            ToolTip.delay: 900
                            ToolTip.text: "Drag to move it next to another panel or out to its own window. Right-click for more."
                            HoverHandler { id: hovh }
                        }
                    }
                }
            }
            Btn {
                id: menuButton
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                iconName: "ellipsis"
                Accessible.name: "Panel menu"
                tip: "Panel menu"
                implicitWidth: 30
                implicitHeight: 24
                leftPadding: 0
                rightPadding: 0
                onClicked: { menu.panel = group.active; menu.popup(menuButton, 0, menuButton.height) }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: Math.max(0, group.panelIds.indexOf(group.active))
            Repeater {
                model: group.panelIds
                delegate: Loader {
                    required property string modelData
                    sourceComponent: group.host.componentFor(modelData)
                }
            }
        }
    }

    // the places a panel can go, without dragging
    Menu {
        id: menu
        property string panel: ""
        MenuItem { text: "Open in its own window"; onTriggered: group.host.floatPanel(menu.panel) }
        MenuItem { text: "Hide this panel"; onTriggered: group.host.hide(menu.panel) }
        MenuSeparator {}
        MenuItem { text: "Dock at the left edge"; onTriggered: group.host.dockEdge(menu.panel, "left", group.windowId) }
        MenuItem { text: "Dock at the right edge"; onTriggered: group.host.dockEdge(menu.panel, "right", group.windowId) }
        MenuItem { text: "Dock at the top"; onTriggered: group.host.dockEdge(menu.panel, "top", group.windowId) }
        MenuItem { text: "Dock at the bottom"; onTriggered: group.host.dockEdge(menu.panel, "bottom", group.windowId) }
        MenuSeparator {}
        Instantiator {
            model: group.host.allGroups().filter(g => g.id !== group.nodeId)
            delegate: MenuItem {
                required property var modelData
                text: "Put in tabs with " + group.host.label(modelData)
                onTriggered: group.host.tabWith(menu.panel, modelData.id)
            }
            onObjectAdded: (i, o) => menu.addItem(o)
            onObjectRemoved: (i, o) => menu.removeItem(o)
        }
    }

    // where a dragged tab would land
    Rectangle {
        readonly property string zone: group.host.dropZone
        visible: group.host.dropGroup === group
        z: 100
        color: Qt.rgba(0.95, 0.79, 0.51, 0.16)
        border.width: 2
        border.color: Theme.accent
        x: zone === "right" ? parent.width / 2 : 0
        y: zone === "bottom" ? parent.height / 2 : 0
        width: zone === "left" || zone === "right" ? parent.width / 2 : parent.width
        height: zone === "top" || zone === "bottom" ? parent.height / 2 : parent.height
        Label {
            anchors.centerIn: parent
            color: Theme.text
            font.pixelSize: 12
            font.bold: true
            text: parent.zone === "center" ? "Add to these tabs" : "Split " + parent.zone
        }
    }
}
