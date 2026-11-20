import QtQuick
import QtQuick.Controls

// The controls that only mean something on the canvas: tools, view toggles, zoom, and what can be
// done to the selection. It sits over the canvas and wraps onto more rows when the panel is
// narrow. Choices that belong to the window (tool, overlays, snapping, rulers, grid, zoom) are
// reported by signals; the commands for the selection go straight to the editor.
Rectangle {
    id: root
    required property var editor
    property string tool: "select"
    property bool overlaysHidden: false
    property bool snapping: true
    property bool rulers: true
    property bool grid: true
    property real zoom: 1

    signal toolChosen(string id)
    signal toggled(string what)
    signal zoomed(string what)

    readonly property var selection: editor.selection ? JSON.parse(editor.selection) : []
    readonly property int count: selection.length
    readonly property bool hasSelection: count > 0

    color: Theme.panel
    implicitHeight: flow.height + 8
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }

    component Sep: Item {
        implicitWidth: 9
        implicitHeight: Theme.row
        Rectangle { anchors.centerIn: parent; width: 1; height: 20; color: Theme.border }
    }
    component Tool: Btn {
        required property string toolId
        required property string label
        required property string key
        property string hint: ""
        implicitWidth: 28
        leftPadding: 0
        rightPadding: 0
        active: root.tool === toolId
        tip: label + (hint !== "" ? ": " + hint : "") + " (" + key + ")"
        onClicked: root.toolChosen(toolId)
    }
    component Toggle: Btn {
        required property string what
        required property bool isOn
        required property string label
        property string key: ""
        implicitWidth: 28
        leftPadding: 0
        rightPadding: 0
        active: isOn
        Accessible.checkable: true
        Accessible.checked: isOn
        tip: label + (key !== "" ? " (" + key + ")" : "")
        onClicked: root.toggled(what)
    }
    component Action: Btn {
        property string label: ""
        property string key: ""
        implicitWidth: 28
        leftPadding: 0
        rightPadding: 0
        tip: label + (key !== "" ? " (" + key + ")" : "")
    }

    Flow {
        id: flow
        x: 6
        y: 4
        width: parent.width - 12
        spacing: 2

        Tool { iconName: "mouse-pointer-2"; toolId: "select"; label: "Select"; key: "V"; hint: "click, Shift-click, drag to move or box-select" }
        Tool { iconName: "hand"; toolId: "hand"; label: "Hand"; key: "H"; hint: "drag to move the view, or hold Space" }
        Tool { iconName: "map-pin"; toolId: "pin"; label: "Note"; key: "C"; hint: "pin a note for the agent on a layer or a region" }
        Sep {}
        Tool { iconName: "square"; toolId: "rect"; label: "Rectangle"; key: "R"; hint: "drag to draw, Shift for a square" }
        Tool { iconName: "circle"; toolId: "ellipse"; label: "Ellipse"; key: "O"; hint: "drag to draw, Shift for a circle" }
        Tool { iconName: "type"; toolId: "text"; label: "Text"; key: "T"; hint: "drag to draw a text box" }
        Tool { iconName: "frame"; toolId: "frame"; label: "Frame"; key: "F"; hint: "drag to draw a frame" }
        Sep {}
        Toggle {
            iconName: "scan-eye"; what: "overlays"; isOn: root.overlaysHidden
            label: root.overlaysHidden ? "Overlays are hidden: showing the clean design. Click to show selection, agent outlines and pins" : "Hide selection, agent outlines, pins and markers to see the clean design"
            key: "⇧H"
        }
        Toggle { iconName: "magnet"; what: "snap"; isOn: root.snapping; label: "Snap to edges and centers while moving (hold ⌘ or Ctrl to skip once)" }
        Toggle { iconName: "ruler"; what: "rulers"; isOn: root.rulers; label: "Rulers" }
        Toggle { iconName: "grid-3x3"; what: "grid"; isOn: root.grid; label: "Pixel grid at high zoom" }
        Sep {}
        Action { iconName: "minus"; label: "Zoom out"; key: "⌘−"; onClicked: root.zoomed("out") }
        Btn {
            text: Math.round(root.zoom * 100) + "%"
            implicitWidth: 52
            leftPadding: 0
            rightPadding: 0
            tip: "Zoom to 100% (⌘0)"
            onClicked: root.zoomed("actual")
        }
        Action { iconName: "plus"; label: "Zoom in"; key: "⌘+"; onClicked: root.zoomed("in") }
        Action { iconName: "maximize"; label: "Fit the frame"; key: "⇧1"; onClicked: root.zoomed("fit") }
        Action { iconName: "scan"; label: "Fit the selection"; key: "⇧2"; onClicked: root.zoomed("selection") }
        Sep {}
        ArrangeGroup { editor: root.editor; group: "align"; count: root.count }
        ArrangeGroup { editor: root.editor; group: "distribute"; count: root.count }
        ArrangeGroup { editor: root.editor; group: "stack"; count: root.count }
        Sep {}
        Action { iconName: "group"; label: "Group"; key: "⌘G"; enabled: root.editor.canGroup === true; onClicked: root.editor.groupSelected() }
        Action { iconName: "ungroup"; label: "Ungroup"; key: "⇧⌘G"; enabled: root.editor.canUngroup === true; onClicked: root.editor.ungroupSelected() }
        Action { iconName: "copy"; label: "Duplicate"; key: "⌘D"; enabled: root.hasSelection; onClicked: root.editor.duplicateSelected() }
    }
}
