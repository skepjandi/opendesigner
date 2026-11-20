import QtQuick

// One row of arrange buttons for the selection, shared by the canvas toolbar and the inspector:
// `align` (six edges and centers), `distribute` (equal gaps, three or more layers) and `stack`
// (a one-shot layout: the layers edge to edge, not a persistent auto-layout). The editor does the
// arithmetic; the buttons only say which command.
Row {
    id: root
    required property var editor
    required property string group
    property int count: 0
    property real gap: 16
    spacing: 2

    readonly property var items: {
        switch (group) {
        case "align": return [
            { icon: "align-start-vertical", tip: "Align left", arg: "left" },
            { icon: "align-center-vertical", tip: "Align horizontal centers", arg: "hcenter" },
            { icon: "align-end-vertical", tip: "Align right", arg: "right" },
            { icon: "align-start-horizontal", tip: "Align top", arg: "top" },
            { icon: "align-center-horizontal", tip: "Align vertical middles", arg: "vmiddle" },
            { icon: "align-end-horizontal", tip: "Align bottom", arg: "bottom" }
        ]
        case "distribute": return [
            { icon: "align-horizontal-distribute-center", tip: "Distribute horizontally (equal gaps, 3 or more layers)", arg: "h" },
            { icon: "align-vertical-distribute-center", tip: "Distribute vertically (equal gaps, 3 or more layers)", arg: "v" }
        ]
        default: return [
            { icon: "columns-2", tip: "Stack side by side with the gap (one-shot, not auto layout)", arg: "row" },
            { icon: "rows-2", tip: "Stack top to bottom with the gap (one-shot, not auto layout)", arg: "column" }
        ]
        }
    }

    function run(arg) {
        if (group === "align") editor.alignSelected(arg)
        else if (group === "distribute") editor.distributeSelected(arg)
        else editor.stackSelected(arg, gap)
    }

    Repeater {
        model: root.items
        delegate: Btn {
            required property var modelData
            iconName: modelData.icon
            tip: modelData.tip
            implicitWidth: 28
            leftPadding: 0
            rightPadding: 0
            enabled: root.group === "align" ? root.count >= 1 : root.group === "distribute" ? root.count >= 3 : root.count >= 2
            onClicked: root.run(modelData.arg)
        }
    }
}
