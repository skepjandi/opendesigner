import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Pick what a workspace is for: a size from the sections (videos, store images, social images,
// web and icons), or your own width, height and frame rate, or leave it to the agent. Nothing is
// required; `picked(format)` is the choice ({id} or {kind, width, height, fps}) or null for "the
// agent decides".
ColumnLayout {
    id: root
    required property var editor
    /// The id (or "custom") of what is chosen now, to highlight.
    property string current: ""
    signal picked(var format)

    readonly property var sections: JSON.parse(editor.formats())
    spacing: 6

    Repeater {
        model: root.sections
        delegate: ColumnLayout {
            required property var modelData
            Layout.fillWidth: true
            spacing: 3
            Label { text: modelData.section; color: Theme.accent; font.pixelSize: 11; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: modelData.formats
                    delegate: Btn {
                        required property var modelData
                        text: modelData.label + "  " + modelData.width + "×" + modelData.height + (modelData.fps ? " · " + modelData.fps + " fps" : "")
                        active: root.current === modelData.id
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: root.picked({id: modelData.id})
                        ToolTip.visible: hovered && modelData.note !== ""
                        ToolTip.text: modelData.note
                    }
                }
            }
        }
    }

    Label { text: "Your own size"; color: Theme.accent; font.pixelSize: 11; font.bold: true }
    RowLayout {
        Layout.fillWidth: true
        spacing: 6
        ComboBox { id: kindBox; model: ["Video", "Image"]; Layout.preferredWidth: 90; implicitHeight: Theme.row }
        Field { id: widthField; Layout.preferredWidth: 70; placeholderText: "width"; text: "1080"; validator: IntValidator { bottom: 16; top: 16384 } }
        Label { text: "×"; color: Theme.muted }
        Field { id: heightField; Layout.preferredWidth: 70; placeholderText: "height"; text: "1920"; validator: IntValidator { bottom: 16; top: 16384 } }
        Field { id: fpsField; visible: kindBox.currentIndex === 0; Layout.preferredWidth: 56; placeholderText: "fps"; text: "30"; validator: IntValidator { bottom: 1; top: 60 } }
        Label { visible: kindBox.currentIndex === 0; text: "fps"; color: Theme.muted }
        Btn {
            text: "Use this size"
            active: root.current === "custom"
            enabled: parseInt(widthField.text) > 0 && parseInt(heightField.text) > 0
            onClicked: root.picked({
                kind: kindBox.currentIndex === 0 ? "video" : "image",
                width: parseInt(widthField.text),
                height: parseInt(heightField.text),
                fps: kindBox.currentIndex === 0 ? parseInt(fpsField.text) || 30 : 0
            })
        }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: 11
        text: "Video sizes are even numbers up to 4096; images up to 16384."
    }
    Btn {
        text: "Let the agent decide"
        active: root.current === ""
        onClicked: root.picked(null)
    }
}
