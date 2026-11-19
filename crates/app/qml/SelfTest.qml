import QtQuick

// Drives the real window through a scripted session when the app is started with
// `--selftest=<empty folder>`. `crates/app/tests/selftest.rs` runs it offscreen and checks the
// outcome. Without the flag this does nothing.
Item {
    id: st
    required property var editor
    required property var canvas
    required property var dock
    required property var grabTarget

    readonly property string folder: {
        for (const a of Qt.application.arguments)
            if (a.indexOf("--selftest=") === 0) return a.substring(11)
        return ""
    }
    readonly property string agentCommand: {
        for (const a of Qt.application.arguments)
            if (a.indexOf("--selftest-agent=") === 0) return a.substring(17)
        return ""
    }
    property var ids: ({})
    // stable updates: a model fed again with a changed list keeps the delegates of what stayed
    property int built: 0
    JsonModel { id: probe }
    Repeater {
        model: probe
        delegate: Item {
            required property string json
            Component.onCompleted: st.built++
        }
    }
    property int n: 0
    property int waits: 0
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
            probe.sync([{ id: "a", v: 1 }, { id: "b", v: 1 }, { id: "c", v: 1 }])
            const first = st.built
            probe.sync([{ id: "a", v: 1 }, { id: "b", v: 2 }, { id: "c", v: 1 }])
            probe.sync([{ id: "c", v: 1 }, { id: "a", v: 1 }, { id: "b", v: 2 }, { id: "d", v: 1 }])
            return check(first === 3, "the first list builds three rows: " + first) &&
                   check(st.built === 4, "only the new row is built; changed and moved rows keep theirs: " + st.built) &&
                   check(JSON.parse(probe.get(2).json).v === 2 && probe.count === 4, "the changed row carries its new value")
        },
        function () {
            // the canvas is never blank while it redraws: the picture in front stays ready
            if (!canvas.pictureReady) return "wait"
            for (let i = 0; i < 3; ++i) {
                editor.renderCanvas(canvas.renderScale)
                if (!check(canvas.front.status === Image.Ready, "the picture in front must stay ready while a new one loads (round " + i + ")")) return false
            }
            return true
        },
        function () {
            return check(Array.isArray(JSON.parse(editor.jobs)), "the progress strip's data is a list")
        },
        function () {
            return check(editor.skillOfferPending(), "an agent is installed and the skill is not, so it is offered") &&
                   check(JSON.parse(editor.answerSkillOffer(false)).length === 0, "not asking again installs nothing") &&
                   check(!editor.skillOfferPending(), "the offer is made once")
        },
        function () {
            editor.startMcp()
            const ok = editor.mcpRunning && JSON.parse(editor.connectRecipes()).length >= 7
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
            editor.setToken("brand.gold", "#F2CA83")
            editor.setNodeProperty(ids.ellipse, "fill", '"{brand.gold}"')
            const row = JSON.parse(editor.tokens).find(t => t.name === "brand.gold")
            return check(row && row.value === "#f2ca83" && row.used_by === 1, "the token should exist with one user: " + editor.tokens) &&
                   check(!editor.messageIsError, "token calls should not error: " + editor.message)
        },
        function () {
            editor.removeToken("brand.gold")
            return check(editor.messageIsError && editor.message.indexOf("used by") >= 0, "a token in use must not be removable") &&
                   check(JSON.parse(editor.tokens).length === 1, "the token should still be there")
        },
        function () {
            editor.addLanguage("ja")
            editor.setNodeProperty(ids.text, "text", '"守る"')
            const row = JSON.parse(editor.languages).find(l => l.locale === "ja")
            editor.select(ids.text)
            return check(editor.language === "ja" && row && row.translated >= 1, "the language should exist and be active: " + editor.message) &&
                   check(insp().text === "守る" && insp().translated === true, "the inspector should show the translation")
        },
        function () {
            const set = JSON.parse(editor.exportAssets("set", "", "tr", 1, false))
            editor.switchLanguage("")
            const back = insp().text !== "守る"
            return check(!set.error && set.files[0].path === "exports/tr-ja-01.png", "an export in a language is named after it: " + JSON.stringify(set)) &&
                   check(back, "the default text must be untouched by a translation") && check(pngReady(), "the canvas should still render")
        },
        function () {
            const id = editor.importImage("file://" + folder + "/fixture.png")
            return check(id !== "", "the image was not imported: " + editor.message) &&
                   check(insp().image === "media/fixture.png", "the image node should point at the media copy") &&
                   check(JSON.parse(editor.unrendered).length === 0, "a readable image needs no placeholder")
        },
        function () {
            const r = JSON.parse(editor.exportAssets("frame", "apple-iphone-6.9", "shot.png", 1, false))
            return check(!r.error && r.files.length === 1 && r.files[0].path === "exports/shot.png" && r.files[0].width === 1320, "validated export failed: " + JSON.stringify(r))
        },
        function () {
            const r = JSON.parse(editor.exportAssets("frame", "google-feature-graphic", "bad.png", 1, false))
            return check(!!r.error && r.error.indexOf("1024x500") >= 0, "a wrong preset must be refused")
        },
        function () {
            const set = JSON.parse(editor.exportAssets("set", "apple-iphone-6.9", "slide", 1, false))
            const pano = JSON.parse(editor.exportAssets("panorama", "", "pano", 2, false))
            return check(!set.error && set.files.length === 1 && set.files[0].path === "exports/slide-01.png", "set export failed: " + JSON.stringify(set)) &&
                   check(!pano.error && pano.files.length === 2 && pano.files[0].width === 660, "panorama export failed: " + JSON.stringify(pano))
        },
        function () {
            const kinds = JSON.parse(editor.deviceKinds())
            const id = editor.addDevice(kinds[0].id)
            const names = JSON.parse(editor.layers).map(l => l.name)
            return check(kinds.length >= 3 && id !== "", "a device frame was not added: " + editor.message) &&
                   check(names.indexOf("Screen") >= 0 && names.indexOf("Body") >= 0, "the device should add a body and a screen")
        },
        function () {
            // the panels can be rearranged and every change leaves a working window
            editor.layoutFloat("video", 120, 90, 420, 520)
            return check(dock.windowIds.length === 1, "floating a panel should give it a window") &&
                   check(dock.groupOf("video") !== dock.groupOf("notes"), "the floated panel left its tabs")
        },
        function () {
            editor.layoutMove("chat", dock.groupOf("canvas"), "right")
            editor.layoutDock("history", "bottom", 0)
            return check(dock.groupOf("chat") !== dock.groupOf("notes"), "chat should have its own group next to the canvas") &&
                   check(dock.groupOf("history") !== dock.groupOf("notes"), "history should be docked at the bottom") &&
                   check(JSON.parse(editor.layout).main.kind === "split", "the main window is a split")
        },
        function () {
            editor.layoutHide("memory")
            const hidden = dock.hidden.indexOf("memory") >= 0
            editor.layoutShow("memory")
            return check(hidden, "a hidden panel should be listed as hidden") &&
                   check(dock.hidden.length === 0 && dock.groupOf("memory") > 0, "a shown panel should come back")
        },
        function () {
            const w = editor.layoutReveal("video")
            return check(w > 0 && w === dock.windowIds[0], "revealing a floated panel names its window") &&
                   check(canvas !== null && canvas.zoom > 0, "the canvas should survive the rearranging") &&
                   check(editor.layoutReveal("notes") === 0, "a docked panel is in the main window")
        },
        function () {
            editor.layoutDockWindow(dock.windowIds[0])
            return check(dock.windowIds.length === 0, "closing a window should put its panel back") &&
                   check(dock.groupOf("video") > 0, "the panel should be docked again")
        },
        function () {
            editor.layoutMove("layers", dock.groupOf("layers"), "left")
            const refused = editor.messageIsError
            editor.layoutReset()
            return check(refused, "splitting a panel from itself should be refused") &&
                   check(JSON.parse(editor.layout).floating.length === 0 && dock.groupOf("chat") === dock.groupOf("notes"), "the layout should reset") &&
                   check(canvas !== null, "the canvas should be back after a reset")
        },
        function () {
            if (agentCommand === "") return true // the chat needs an agent; the test passes one
            editor.chatStart(agentCommand)
            return check(!editor.messageIsError, "the chat should start: " + editor.message)
        },
        function () {
            if (agentCommand === "") return true
            const c = JSON.parse(editor.chat)
            if (c.status === "starting") return "wait"
            return check(c.status === "ready", "the chat did not connect: " + editor.chat) && check(editor.mcpRunning, "the chat turns the agent connection on")
        },
        function () {
            if (agentCommand === "") return true
            editor.chatSend("make the headline bigger")
            return true
        },
        function () {
            if (agentCommand === "") return true
            const p = JSON.parse(editor.chat).permission
            if (!p) return "wait"
            editor.chatAnswer(p.request, p.options[0].id)
            return true
        },
        function () {
            if (agentCommand === "") return true
            const c = JSON.parse(editor.chat)
            if (c.status !== "ready") return "wait"
            const said = c.messages.filter(m => m.role === "agent").map(m => m.text).join("")
            const plan = c.plan.map(p => p.content + "=" + p.status).join(",")
            const ok = check(said === "Hello world [fs refused] [chose yes]", "the agent's reply: " + said) &&
                       check(plan === "Read the guidance=completed,Make it bigger=in_progress", "the agent's plan: " + plan)
            editor.chatStop()
            return ok && check(JSON.parse(editor.chat).status === "off", "the chat should stop")
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
            if (ok === "wait") {
                st.waits++
                if (st.waits > 80) { st.fail("step " + i + " waited too long"); return }
                timer.restart()
                return
            }
            st.waits = 0
            if (!ok) return
            if (i === st.steps.length - 2) return // the grab step advances itself
            st.n++
            timer.restart()
        }
    }

    Component.onCompleted: { if (folder !== "") timer.start() }
}
