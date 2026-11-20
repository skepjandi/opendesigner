import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The options the agent offers (offer_variants): one card each, so the user can compare and pick
// without the canvas. The pick is stored and sent to the agent; the panel only presents.
Rectangle {
    id: root
    required property var editor
    color: Theme.panel

    signal showOnCanvas(string id)
    signal askForDifferent(string text)

    readonly property var showcase: editor.showcase ? JSON.parse(editor.showcase) : null
    readonly property var set: showcase ? showcase.variants : null
    readonly property int columns: Math.max(1, Math.floor((scroll.availableWidth - 20) / 230))
    readonly property real cardWidth: columns > 0 ? (scroll.availableWidth - 20 - (columns - 1) * 10) / columns : 200

    function thumb(o) {
        if (!o.exists) return ""
        if (o.kind === "frame") return editor.renderFrameImage(o.target, 480)
        if (o.kind === "video") return editor.videoPreview(o.target, 0, 480)
        return o.file ? "file://" + o.file : ""
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        ColumnLayout {
            visible: !root.set
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 8
            Icon { name: "layers"; size: 22; color: Theme.muted }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.text
                font.pixelSize: Theme.fontSize
                font.bold: true
                text: "No options to choose from"
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: "When you ask the agent for directions or options first (for example: show me three small directions), each one appears here as a card. Pick one and the agent continues with it. Nothing is built on the canvas for you to dig through."
            }
        }

        ColumnLayout {
            visible: !!root.set
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.topMargin: 10
            spacing: 2
            RowLayout {
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: Theme.text
                    font.pixelSize: 14
                    font.bold: true
                    text: root.set ? root.set.title : ""
                }
                Label {
                    color: Theme.muted
                    font.pixelSize: 11
                    text: root.set ? "Round " + root.set.round : ""
                }
                Btn { iconName: "x"; tip: "Dismiss these options"; onClicked: root.editor.dismissVariants() }
            }
            Label {
                visible: !!root.set && !!root.set.question
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.pixelSize: Theme.fontSize
                text: root.set && root.set.question ? root.set.question : ""
            }
            RowLayout {
                Layout.topMargin: 4
                Label {
                    color: root.set && root.set.picked ? Theme.success : Theme.muted
                    font.pixelSize: 11
                    text: root.set && root.set.picked ? "You picked an option. The agent continues with it." : "Pick one. The agent waits for you."
                }
                Item { Layout.fillWidth: true }
                Btn {
                    text: "Ask for different options"
                    onClicked: root.askForDifferent("None of these options is right. Offer me different ones" + (root.set ? " for \"" + root.set.title + "\"" : "") + ".")
                }
            }
        }

        ScrollView {
            id: scroll
            visible: !!root.set
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth

            Flow {
                x: 10
                width: scroll.availableWidth - 20
                spacing: 10
                bottomPadding: 10

                Repeater {
                    model: root.set ? root.set.options : []
                    delegate: Rectangle {
                        id: card
                        required property var modelData
                        readonly property bool picked: !!root.set && root.set.picked === modelData.id
                        width: root.cardWidth
                        height: cardColumn.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.panelRaised
                        border.width: picked ? 2 : 1
                        border.color: picked ? Theme.accent : Theme.border

                        ColumnLayout {
                            id: cardColumn
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 8
                            spacing: 6

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 150
                                radius: 4
                                color: Theme.field
                                clip: true
                                Image {
                                    anchors.fill: parent
                                    anchors.margins: 4
                                    source: { root.editor.log; return root.thumb(card.modelData) }
                                    fillMode: Image.PreserveAspectFit
                                    asynchronous: false
                                    cache: false
                                }
                                Label {
                                    anchors.centerIn: parent
                                    visible: !card.modelData.exists
                                    color: Theme.danger
                                    font.pixelSize: 11
                                    text: "No longer in the project"
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Icon { visible: card.picked; name: "check"; size: 14; color: Theme.accent }
                                Label {
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    color: Theme.text
                                    font.pixelSize: Theme.fontSize
                                    font.bold: true
                                    text: card.modelData.label
                                }
                            }
                            Label {
                                visible: !!card.modelData.note
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                                color: Theme.muted
                                font.pixelSize: 11
                                text: card.modelData.note ? card.modelData.note : ""
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Btn {
                                    Layout.fillWidth: true
                                    primary: !card.picked
                                    active: card.picked
                                    iconName: card.picked ? "check" : ""
                                    text: card.picked ? "Picked" : "Pick this one"
                                    enabled: card.modelData.exists
                                    onClicked: root.editor.pickVariant(card.modelData.id)
                                }
                                Btn {
                                    visible: card.modelData.kind === "frame"
                                    enabled: card.modelData.exists
                                    iconName: "scan-eye"
                                    tip: "Show on canvas"
                                    onClicked: root.showOnCanvas(card.modelData.target)
                                }
                            }
                        }
                    }
                }
            }
        }
        Item { visible: !root.set; Layout.fillHeight: true }
    }
}
