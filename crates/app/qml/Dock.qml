import QtQuick

// The controller of the panel layout. The Rust side (`layout.rs`) owns the arrangement and checks
// every change; this reads it, answers the questions the dock items ask, and turns a dragged tab
// into one of those changes. Nothing here decides what is allowed.
QtObject {
    id: dock
    required property var editor
    /// The window the dock sits in, for placing new windows near it.
    required property var mainWindow
    /// panel id -> Component, filled in by main.qml (it owns what each panel is wired to).
    property var components: ({})
    readonly property var panelTitles: ({
        layers: "Layers", canvas: "Canvas", inspector: "Inspector", notes: "Notes", history: "History",
        memory: "Memory", run: "Run", web: "Web", devices: "Devices", video: "Video", connect: "Connect", chat: "Chat", variants: "Variants", preview: "Preview"
    })
    readonly property var panelOrder: ["layers", "canvas", "inspector", "notes", "history", "memory", "run", "web", "devices", "video", "connect", "chat", "variants", "preview"]

    readonly property var layout: editor.layout !== "" ? JSON.parse(editor.layout) : null
    readonly property int mainRootId: layout ? layout.main.id : 0
    // ids as strings, so a change to sizes or tabs does not rebuild the windows
    readonly property string windowKey: layout ? layout.floating.map(f => f.id).join(",") : ""
    readonly property var windowIds: windowKey === "" ? [] : windowKey.split(",").map(Number)
    readonly property var hidden: layout ? layout.hidden : []
    readonly property int openNotes: editor.annotations ? JSON.parse(editor.annotations).filter(n => n.status === "open").length : 0

    // ---- reading the layout ----
    function find(n, id) {
        if (n.id === id) return n
        if (n.kind === "split") for (const c of n.children) { const r = find(c, id); if (r) return r }
        return null
    }
    function node(id) {
        if (!layout) return null
        let r = find(layout.main, id)
        for (let i = 0; !r && i < layout.floating.length; ++i) r = find(layout.floating[i].root, id)
        return r
    }
    function window(id) { return layout ? (layout.floating.find(f => f.id === id) || null) : null }
    function groupsIn(n, out) {
        if (n.kind === "group") out.push(n)
        else for (const c of n.children) groupsIn(c, out)
        return out
    }
    function allGroups() {
        if (!layout) return []
        const out = groupsIn(layout.main, [])
        for (const f of layout.floating) groupsIn(f.root, out)
        return out
    }
    function groupOf(panel) {
        for (const g of allGroups()) if (g.panels.indexOf(panel) >= 0) return g.id
        return 0
    }
    function title(panel) {
        return panelTitles[panel] + (panel === "notes" && openNotes > 0 ? " (" + openNotes + ")" : "")
    }
    function windowTitle(id) {
        const w = window(id)
        if (!w) return "OpenDesigner"
        return groupsIn(w.root, []).map(g => g.panels.map(p => panelTitles[p]).join(" · ")).join(" | ")
    }
    function label(g) { return g.panels.map(p => panelTitles[p]).join(" · ") }
    function componentFor(panel) { return components[panel] || null }

    // ---- changing it (each is one call to the Rust side) ----
    function activate(group, panel) { editor.layoutActive(group, panel) }
    function floatPanel(panel) { editor.layoutFloat(panel, mainWindow.x + 100, mainWindow.y + 100, 440, 560) }
    function dockEdge(panel, edge, windowId) { editor.layoutDock(panel, edge, windowId) }
    function hide(panel) { editor.layoutHide(panel) }
    function show(panel) { editor.layoutShow(panel) }
    function dockWindow(id) { editor.layoutDockWindow(id) }
    function commitWeights(split, weights) { editor.layoutWeights(split, JSON.stringify(weights)) }
    function setGeometry(id, x, y, w, h) { editor.layoutGeometry(id, x, y, w, h) }
    function reset() { editor.layoutReset() }
    function tabWith(panel, group) { editor.layoutMove(panel, group, "center") }

    // ---- dragging a tab ----
    property var groups: []
    property var dropGroup: null
    property string dropZone: ""
    property string dragging: ""
    function registerGroup(g) { groups = groups.concat([g]) }
    function unregisterGroup(g) { groups = groups.filter(x => x !== g) }
    function groupAt(gp) {
        // later windows are on top of earlier ones
        for (let i = groups.length - 1; i >= 0; --i) {
            const g = groups[i]
            if (!g || !g.visible) continue
            const o = g.mapToGlobal(0, 0)
            if (gp.x >= o.x && gp.x < o.x + g.width && gp.y >= o.y && gp.y < o.y + g.height) return g
        }
        return null
    }
    function zoneFor(g, gp) {
        const o = g.mapToGlobal(0, 0)
        const rx = (gp.x - o.x) / g.width, ry = (gp.y - o.y) / g.height
        if (gp.y - o.y < 32 || (rx > 0.25 && rx < 0.75 && ry > 0.25 && ry < 0.75)) return "center"
        const d = { left: rx, right: 1 - rx, top: ry, bottom: 1 - ry }
        let best = "left"
        for (const k of ["right", "top", "bottom"]) if (d[k] < d[best]) best = k
        return best
    }
    function dragStart(panel) { dragging = panel; dropGroup = null; dropZone = "" }
    function dragMove(gp) {
        const g = groupAt(gp)
        // dropping a panel on the group it is alone in would change nothing
        if (!g || (g.panelIds.length === 1 && g.panelIds[0] === dragging)) { dropGroup = null; dropZone = ""; return }
        dropGroup = g
        dropZone = zoneFor(g, gp)
    }
    function dragEnd(gp) {
        const panel = dragging, g = dropGroup, zone = dropZone
        const over = groupAt(gp)
        dragging = ""; dropGroup = null; dropZone = ""
        if (panel === "") return
        // the tab that started the drag is destroyed when the layout changes, so change it after
        // the handler that is running has returned
        Qt.callLater(() => {
            if (g) editor.layoutMove(panel, g.nodeId, zone)
            else if (!over) editor.layoutFloat(panel, gp.x - 60, gp.y - 14, 440, 560)
        })
    }
}
