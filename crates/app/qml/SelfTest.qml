import QtQuick

// Drives the real window through a scripted session when the app is started with
// `--selftest=<empty folder>`. `crates/app/tests/selftest.rs` runs it offscreen and checks the
// outcome. Without the flag this does nothing.
Item {
    id: st
    required property var editor
    required property var canvas
    required property var grabTarget

    readonly property string folder: {
        for (const a of Qt.application.arguments)
            if (a.indexOf("--selftest=") === 0) return a.substring(11)
        return ""
    }
    property var ids: ({})
    property int n: 0
    visible: false

    function fail(msg) { console.log("SELFTEST FAIL: " + msg); Qt.exit(1) }
    function check(cond, msg) { if (!cond) { fail(msg); return false } return true }
    function pngReady() { return editor.canvas.indexOf("data:image/png;base64,") === 0 && editor.canvas.length > 400 }
    function insp() { return JSON.parse(editor.inspector) }

    readonly property var steps: [
        function () {
            editor.newProject(folder + "/p.od", "Selftest", 1320, 2868)
            return check(editor.hasProject, "project did not open") &&
                   check(JSON.parse(editor.frames).length === 1, "expected one frame")
        },
        function () {
            ids.rect = editor.createNode("rect", 100, 200, 500, 300)
            ids.ellipse = editor.createNode("ellipse", 700, 200, 300, 300)
            ids.text = editor.createNode("text", 100, 700, 800, 200)
            return check(ids.rect !== "" && ids.ellipse !== "" && ids.text !== "", "shapes were not created") &&
                   check(JSON.parse(editor.layers).length === 5, "layers should list the page, the frame and three shapes") &&
                   check(editor.hitTest(150, 250) === ids.rect, "hit test should find the rectangle") &&
                   check(JSON.parse(editor.unrendered).length === 0, "text is drawn by the renderer, so nothing is left to overlay")
        },
        function () { canvas.fit(); return true },
        function () {
            return check(pngReady(), "the canvas was not rendered") &&
                   check(canvas.zoom > 0.05 && canvas.zoom < 1, "the frame should be fitted to the window")
        },
        function () {
            editor.select(ids.rect)
            editor.setNodeProperty(ids.rect, "fill", '"#ff0000"')
            return check(insp().fill === "#ff0000", "inspector edit did not apply") &&
                   check(JSON.parse(editor.selectionBounds).width === 500, "selection bounds missing")
        },
        function () {
            editor.setNodeProperty(ids.rect, "opacity", "7")
            return check(editor.messageIsError && editor.message.indexOf("opacity") >= 0, "a bad value should be reported with its reason")
        },
        function () {
            editor.undo()
            const undone = insp().fill !== "#ff0000"
            editor.redo()
            return check(undone, "undo did not revert the fill") && check(insp().fill === "#ff0000", "redo did not reapply")
        },
        function () {
            editor.moveNode(ids.rect, 120, 220)
            editor.resizeNode(ids.rect, 120, 220, 400, 250)
            const b = JSON.parse(editor.selectionBounds)
            return check(b.x === 120 && b.width === 400 && b.height === 250, "move/resize did not apply")
        },
        function () {
            const pin = editor.addPin("make this bigger", ids.rect)
            return check(pin > 0, "pin was not created") &&
                   check(JSON.parse(editor.annotations).length === 1, "pin missing from annotations") &&
                   check(JSON.parse(editor.annotations)[0].bounds.width === 400, "pin should carry the node's bounds")
        },
        function () {
            editor.startMcp()
            const ok = editor.mcpRunning && editor.connectCommand().indexOf("claude mcp add") === 0
            editor.stopMcp()
            return check(ok, "agent connection did not start") && check(!editor.mcpRunning, "agent connection did not stop")
        },
        function () {
            editor.checkpoint("selftest")
            editor.moveNode(ids.rect, 10, 10)
            editor.restore(1)
            return check(JSON.parse(editor.selectionBounds).x === 120, "restore did not bring the position back")
        },
        function () {
            const id = editor.importImage("file://" + folder + "/fixture.png")
            return check(id !== "", "the image was not imported: " + editor.message) &&
                   check(insp().image === "media/fixture.png", "the image node should point at the media copy") &&
                   check(JSON.parse(editor.unrendered).length === 0, "a readable image needs no placeholder")
        },
        function () {
            const r = JSON.parse(editor.exportFrame("apple-iphone-6.9", "shot.png"))
            return check(!r.error && r.path === "exports/shot.png" && r.width === 1320, "validated export failed: " + JSON.stringify(r))
        },
        function () {
            const r = JSON.parse(editor.exportFrame("google-feature-graphic", "bad.png"))
            return check(!!r.error && r.error.indexOf("1024x500") >= 0, "a wrong preset must be refused")
        },
        function () {
            grabTarget.grabToImage(function (result) {
                if (!result.saveToFile(folder + "/window.png")) st.fail("could not save the window image")
                st.n++
                st.timer.restart()
            })
            timer.stop()
            return true
        },
        function () { console.log("SELFTEST OK"); Qt.quit(); return true }
    ]

    property Timer timer: Timer {
        interval: 250
        repeat: false
        onTriggered: {
            if (st.n >= st.steps.length) return
            const i = st.n
            let ok = false
            try { ok = st.steps[i]() } catch (e) { st.fail("step " + i + " threw: " + e); return }
            if (!ok) return
            if (i === st.steps.length - 2) return // the grab step advances itself
            st.n++
            timer.restart()
        }
    }

    Component.onCompleted: { if (folder !== "") timer.start() }
}
