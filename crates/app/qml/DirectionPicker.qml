import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// What the work should look like, before the agent starts: a look, colors, type, mood, motion,
// layout, imagery, who it is for, and how the agent should work. The picks become a block in
// the prompt (`od_agent::direction`).
Dialog {
    id: picker
    required property var editor
    signal picked(string text)

    readonly property var catalog: JSON.parse(editor.directionCatalog())
    property var styles: []
    property var moods: []
    property var imagery: []
    property string palette: ""
    property string typePair: ""
    property string motion: ""
    property string density: ""
    property string customColors: ""

    function toggle(list, id) {
        const at = list.indexOf(id)
        if (at >= 0) list.splice(at, 1)
        else list.push(id)
    }
    function selection() {
        return {
            styles: styles, moods: moods, imagery: imagery,
            palette: palette, type_pair: typePair, motion: motion, density: density,
            custom_colors: customColors,
            audience: audienceField.text, notes: notesField.text,
            research: researchBox.checked, options_first: optionsBox.checked, remember: rememberBox.checked
        }
    }
    function apply() {
        picker.picked(editor.directionPrompt(JSON.stringify(picker.selection())))
        picker.close()
    }

    title: "Direction"
    modal: true
    width: Math.min(720, parent ? parent.width - 60 : 720)
    height: Math.min(680, parent ? parent.height - 60 : 680)
    standardButtons: Dialog.Cancel
    onOpened: {
        if (picker.styles.length === 0) picker.styles = ["swiss"]
        if (picker.palette === "") picker.palette = "midnight-gold"
        if (picker.typePair === "") picker.typePair = "helvetica"
        if (picker.motion === "") picker.motion = "standard"
        if (picker.density === "") picker.density = "balanced"
    }
    background: Rectangle { radius: Theme.radius; color: Theme.panelRaised; border.width: 1; border.color: Theme.border }
    header: Label { text: picker.title; color: Theme.text; font.pixelSize: 15; font.bold: true; padding: 14 }

    contentItem: ScrollView {
        id: scroll
        contentWidth: availableWidth
        contentHeight: column.implicitHeight + 8
        clip: true
        ScrollBar.vertical: ScrollBar {}
        ColumnLayout {
            id: column
            width: scroll.availableWidth
            spacing: 10

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: 11
                text: "Everything here goes into the prompt as your instructions. The agent follows them; the product's own facts still win over taste."
            }

            Label { text: "Look"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.styles
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.styles.indexOf(modelData.id) >= 0
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.toggle(picker.styles, modelData.id)
                        ToolTip.visible: hovered
                        ToolTip.text: modelData.note
                    }
                }
            }

            Label { text: "Colors"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.palettes
                    delegate: Btn {
                        required property var modelData
                        text: "■ " + modelData.name
                        active: picker.palette === modelData.id && picker.customColors === ""
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: { picker.palette = modelData.id; picker.customColors = "" }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Your own"; color: Theme.muted; font.pixelSize: 11 }
                Field {
                    id: customColorsField
                    Layout.fillWidth: true
                    placeholderText: "hex colors, e.g. #0b1020 #e8b86a"
                    onEditingFinished: {
                        if (text.trim() !== "") { picker.customColors = text.trim(); picker.palette = "" }
                    }
                    onTextChanged: if (text.trim() !== "") { picker.customColors = text.trim(); picker.palette = "" }
                }
            }
            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

            Label { text: "Type"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.types
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.typePair === modelData.id
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.typePair = modelData.id
                    }
                }
            }

            Label { text: "Mood"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.moods
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.moods.indexOf(modelData.id) >= 0
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.toggle(picker.moods, modelData.id)
                    }
                }
            }

            Label { text: "Motion"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.motion
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.motion === modelData.id
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.motion = modelData.id
                    }
                }
            }

            Label { text: "Layout"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.density
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.density === modelData.id
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.density = modelData.id
                    }
                }
            }

            Label { text: "Imagery"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: picker.catalog.imagery
                    delegate: Btn {
                        required property var modelData
                        text: modelData.name
                        active: picker.imagery.indexOf(modelData.id) >= 0
                        implicitHeight: 26
                        font.pixelSize: 11
                        onClicked: picker.toggle(picker.imagery, modelData.id)
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.border }

            Label { text: "Who is it for, and what should it do?"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Field { id: audienceField; Layout.fillWidth: true; placeholderText: "for example: indie developers who want their notes in Git" }

            Label { text: "Anything else"; color: Theme.accent; font.pixelSize: 12; font.bold: true }
            Field { id: notesField; Layout.fillWidth: true; placeholderText: "for example: no stock photos, keep the logo small" }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                CheckBox {
                    id: researchBox
                    text: "Look for good skills and references first"
                    font.pixelSize: 11
                    contentItem: Text { text: parent.text; font: parent.font; color: Theme.text; leftPadding: parent.indicator.width + 4; verticalAlignment: Text.AlignVCenter }
                }
                CheckBox {
                    id: optionsBox
                    text: "Show me three small directions first, then wait for my pick"
                    font.pixelSize: 11
                    contentItem: Text { text: parent.text; font: parent.font; color: Theme.text; leftPadding: parent.indicator.width + 4; verticalAlignment: Text.AlignVCenter }
                }
                CheckBox {
                    id: rememberBox
                    text: "Remember these choices for the next task"
                    font.pixelSize: 11
                    contentItem: Text { text: parent.text; font: parent.font; color: Theme.text; leftPadding: parent.indicator.width + 4; verticalAlignment: Text.AlignVCenter }
                }
            }
        }
    }
    footer: RowLayout {
        spacing: 8
        Item { Layout.fillWidth: true }
        Btn { text: "Cancel"; onClicked: picker.close() }
        Btn { text: "Add to the prompt"; primary: true; onClicked: picker.apply() }
    }
}
